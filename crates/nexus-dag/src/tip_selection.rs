//! Tip selection algorithms for DAG consensus
//!
//! The tip selection algorithm determines which vertices a new transaction
//! should reference as parents. This is crucial for:
//! - Transaction ordering
//! - Preventing double-spending
//! - Ensuring convergence of the DAG

use crate::{Dag, VertexStatus};
use nexus_primitives::{Hash, Height, Weight};
use rand::prelude::*;
use rand::seq::SliceRandom;
use std::collections::HashSet;

/// Tip selection strategy
#[derive(Clone, Debug)]
pub enum TipSelectionStrategy {
    /// Random selection from current tips
    Random,
    
    /// Weighted random based on vertex weight
    WeightedRandom,
    
    /// Select tips with highest weight (most confirmed)
    HighestWeight,
    
    /// MCMC-based selection (similar to IOTA)
    MarkovChainMonteCarlo { alpha: f64, depth: usize },
    
    /// Hybrid: prioritize recent tips with weight consideration
    Hybrid { recency_weight: f64, confirmation_weight: f64 },
}

/// Result of tip selection
#[derive(Clone, Debug)]
pub struct TipSelectionResult {
    /// Selected parent hashes
    pub parents: Vec<Hash>,
    
    /// Calculated height for new vertex
    pub height: Height,
    
    /// Selection algorithm used
    pub strategy: String,
    
    /// Selection metrics
    pub metrics: TipSelectionMetrics,
}

#[derive(Clone, Debug, Default)]
pub struct TipSelectionMetrics {
    pub tips_evaluated: usize,
    pub selection_time_us: u64,
    pub weight_variance: f64,
}

/// Tip selector implementation
pub struct TipSelector {
    strategy: TipSelectionStrategy,
    min_parents: usize,
    max_parents: usize,
}

impl TipSelector {
    pub fn new(strategy: TipSelectionStrategy) -> Self {
        Self {
            strategy,
            min_parents: 1,
            max_parents: 8, // From MAX_PARENTS constant
        }
    }
    
    pub fn with_parent_limits(mut self, min: usize, max: usize) -> Self {
        self.min_parents = min;
        self.max_parents = max.min(nexus_primitives::MAX_PARENTS);
        self
    }
    
    /// Select tips from the DAG
    pub fn select(&self, dag: &Dag) -> TipSelectionResult {
        let start = std::time::Instant::now();
        
        let tips = dag.get_tips();
        let tips_evaluated = tips.len();
        
        let parents = match &self.strategy {
            TipSelectionStrategy::Random => {
                self.random_selection(&tips)
            }
            TipSelectionStrategy::WeightedRandom => {
                self.weighted_random_selection(dag, &tips)
            }
            TipSelectionStrategy::HighestWeight => {
                self.highest_weight_selection(dag, &tips)
            }
            TipSelectionStrategy::MarkovChainMonteCarlo { alpha, depth } => {
                self.mcmc_selection(dag, &tips, *alpha, *depth)
            }
            TipSelectionStrategy::Hybrid { recency_weight, confirmation_weight } => {
                self.hybrid_selection(dag, &tips, *recency_weight, *confirmation_weight)
            }
        };
        
        // Calculate height for new vertex
        let height = parents.iter()
            .filter_map(|h| dag.get_vertex(h).map(|v| v.height))
            .max()
            .map(|h| h + 1)
            .unwrap_or(0);
        
        let selection_time_us = start.elapsed().as_micros() as u64;
        
        TipSelectionResult {
            parents,
            height,
            strategy: format!("{:?}", self.strategy),
            metrics: TipSelectionMetrics {
                tips_evaluated,
                selection_time_us,
                weight_variance: 0.0, // Could calculate if needed
            },
        }
    }
    
    /// Simple random selection
    fn random_selection(&self, tips: &[Hash]) -> Vec<Hash> {
        let mut rng = thread_rng();
        let count = self.select_count(tips.len());
        
        tips.choose_multiple(&mut rng, count)
            .cloned()
            .collect()
    }
    
    /// Weighted random selection based on vertex weight
    fn weighted_random_selection(&self, dag: &Dag, tips: &[Hash]) -> Vec<Hash> {
        let mut rng = thread_rng();
        let count = self.select_count(tips.len());
        
        // Calculate weights
        let weights: Vec<(Hash, f64)> = tips.iter()
            .map(|h| {
                let weight = dag.calculate_weight(h) as f64;
                (*h, weight)
            })
            .collect();
        
        let total_weight: f64 = weights.iter().map(|(_, w)| *w).sum();
        
        if total_weight == 0.0 {
            return self.random_selection(tips);
        }
        
        // Weighted selection without replacement
        let mut selected = Vec::with_capacity(count);
        let mut remaining: Vec<(Hash, f64)> = weights;
        
        for _ in 0..count {
            if remaining.is_empty() {
                break;
            }
            
            let current_total: f64 = remaining.iter().map(|(_, w)| *w).sum();
            let threshold = rng.gen::<f64>() * current_total;
            
            let mut cumulative = 0.0;
            let mut selected_idx = 0;
            
            for (idx, (_, weight)) in remaining.iter().enumerate() {
                cumulative += *weight;
                if cumulative >= threshold {
                    selected_idx = idx;
                    break;
                }
            }
            
            let (hash, _) = remaining.remove(selected_idx);
            selected.push(hash);
        }
        
        selected
    }
    
    /// Select tips with highest weight
    fn highest_weight_selection(&self, dag: &Dag, tips: &[Hash]) -> Vec<Hash> {
        let count = self.select_count(tips.len());
        
        let mut weighted: Vec<(Hash, Weight)> = tips.iter()
            .map(|h| (*h, dag.calculate_weight(h)))
            .collect();
        
        weighted.sort_by(|a, b| b.1.cmp(&a.1));
        
        weighted.into_iter()
            .take(count)
            .map(|(h, _)| h)
            .collect()
    }
    
    /// MCMC-based tip selection (Markov Chain Monte Carlo)
    /// Similar to IOTA's approach but adapted for our DAG
    fn mcmc_selection(
        &self,
        dag: &Dag,
        tips: &[Hash],
        alpha: f64,
        max_depth: usize,
    ) -> Vec<Hash> {
        let mut rng = thread_rng();
        let count = self.select_count(tips.len());
        let mut selected = Vec::with_capacity(count);
        
        for _ in 0..count {
            // Start random walk from a random starting point
            let start_idx = rng.gen_range(0..tips.len());
            let mut current = tips[start_idx];
            
            // Walk towards the genesis, then back to tips
            for _ in 0..max_depth {
                if let Some(vertex) = dag.get_vertex(&current) {
                    if vertex.parents.is_empty() {
                        break; // Reached genesis
                    }
                    
                    // Calculate transition probabilities
                    let parent_weights: Vec<(Hash, f64)> = vertex.parents.iter()
                        .map(|p| {
                            let weight = dag.calculate_weight(p) as f64;
                            (*p, (weight * alpha).exp())
                        })
                        .collect();
                    
                    let total: f64 = parent_weights.iter().map(|(_, w)| *w).sum();
                    
                    if total == 0.0 {
                        break;
                    }
                    
                    // Select next vertex
                    let threshold = rng.gen::<f64>() * total;
                    let mut cumulative = 0.0;
                    
                    for (hash, weight) in parent_weights {
                        cumulative += weight;
                        if cumulative >= threshold {
                            current = hash;
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
            
            // Now walk back towards tips
            let mut walk_result = current;
            for _ in 0..max_depth {
                let children = dag.get_children(&walk_result);
                
                if children.is_empty() {
                    break; // Reached a tip
                }
                
                // Select child with probability proportional to weight
                let child_weights: Vec<(Hash, f64)> = children.iter()
                    .map(|c| {
                        let weight = dag.calculate_weight(c) as f64;
                        (*c, (weight * alpha).exp())
                    })
                    .collect();
                
                let total: f64 = child_weights.iter().map(|(_, w)| *w).sum();
                
                if total == 0.0 {
                    break;
                }
                
                let threshold = rng.gen::<f64>() * total;
                let mut cumulative = 0.0;
                
                for (hash, weight) in child_weights {
                    cumulative += weight;
                    if cumulative >= threshold {
                        walk_result = hash;
                        break;
                    }
                }
            }
            
            if !selected.contains(&walk_result) {
                selected.push(walk_result);
            }
        }
        
        // Ensure minimum parents
        while selected.len() < self.min_parents && !tips.is_empty() {
            let additional = tips.choose(&mut rng).unwrap();
            if !selected.contains(additional) {
                selected.push(*additional);
            }
        }
        
        selected
    }
    
    /// Hybrid selection considering both recency and confirmation
    fn hybrid_selection(
        &self,
        dag: &Dag,
        tips: &[Hash],
        recency_weight: f64,
        confirmation_weight: f64,
    ) -> Vec<Hash> {
        let mut rng = thread_rng();
        let count = self.select_count(tips.len());
        
        // Calculate scores
        let max_height = dag.latest_height();
        
        let scores: Vec<(Hash, f64)> = tips.iter()
            .filter_map(|h| {
                dag.get_vertex(h).map(|v| {
                    let recency_score = (v.height as f64) / (max_height.max(1) as f64);
                    let weight = dag.calculate_weight(h);
                    let confirmation_score = (weight as f64).ln().max(0.0);
                    
                    let total_score = recency_weight * recency_score 
                        + confirmation_weight * confirmation_score;
                    
                    (*h, total_score)
                })
            })
            .collect();
        
        if scores.is_empty() {
            return self.random_selection(tips);
        }
        
        // Softmax-style selection
        let max_score = scores.iter().map(|(_, s)| *s).fold(f64::NEG_INFINITY, f64::max);
        let exp_scores: Vec<(Hash, f64)> = scores.iter()
            .map(|(h, s)| (*h, (s - max_score).exp()))
            .collect();
        
        let _total: f64 = exp_scores.iter().map(|(_, s)| *s).sum();
        
        let mut selected = Vec::with_capacity(count);
        let mut remaining = exp_scores;
        
        for _ in 0..count {
            if remaining.is_empty() {
                break;
            }
            
            let current_total: f64 = remaining.iter().map(|(_, s)| *s).sum();
            let threshold = rng.gen::<f64>() * current_total;
            
            let mut cumulative = 0.0;
            let mut selected_idx = 0;
            
            for (idx, (_, score)) in remaining.iter().enumerate() {
                cumulative += *score;
                if cumulative >= threshold {
                    selected_idx = idx;
                    break;
                }
            }
            
            let (hash, _) = remaining.remove(selected_idx);
            selected.push(hash);
        }
        
        selected
    }
    
    /// Determine how many parents to select
    fn select_count(&self, available: usize) -> usize {
        if available == 0 {
            return 0;
        }
        
        // Target 2 parents for optimal DAG structure
        let target = 2.min(available).max(self.min_parents);
        target.min(self.max_parents)
    }
}

/// Utility function to validate parent selection
pub fn validate_parents(
    dag: &Dag,
    parents: &[Hash],
) -> Result<(), TipValidationError> {
    if parents.is_empty() {
        return Err(TipValidationError::NoParents);
    }
    
    if parents.len() > nexus_primitives::MAX_PARENTS {
        return Err(TipValidationError::TooManyParents);
    }
    
    // Check for duplicates
    let unique: HashSet<_> = parents.iter().collect();
    if unique.len() != parents.len() {
        return Err(TipValidationError::DuplicateParents);
    }
    
    // Check all parents exist and are confirmed
    for parent in parents {
        match dag.get_status(parent) {
            Some(VertexStatus::Confirmed) | Some(VertexStatus::Finalized) => {}
            Some(status) => {
                return Err(TipValidationError::ParentNotConfirmed(*parent, status));
            }
            None => {
                return Err(TipValidationError::ParentNotFound(*parent));
            }
        }
    }
    
    // Check for conflicting parents (parents shouldn't be ancestors of each other)
    for (i, p1) in parents.iter().enumerate() {
        for p2 in parents.iter().skip(i + 1) {
            if dag.is_ancestor(p1, p2) || dag.is_ancestor(p2, p1) {
                return Err(TipValidationError::ConflictingParents(*p1, *p2));
            }
        }
    }
    
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum TipValidationError {
    #[error("No parents selected")]
    NoParents,
    
    #[error("Too many parents (max {})", nexus_primitives::MAX_PARENTS)]
    TooManyParents,
    
    #[error("Duplicate parents in selection")]
    DuplicateParents,
    
    #[error("Parent not found: {0:?}")]
    ParentNotFound(Hash),
    
    #[error("Parent not confirmed: {0:?} (status: {1:?})")]
    ParentNotConfirmed(Hash, VertexStatus),
    
    #[error("Conflicting parents: {0:?} and {1:?} are ancestors")]
    ConflictingParents(Hash, Hash),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DagConfig, Vertex};
    
    fn setup_dag() -> Dag {
        let dag = Dag::new(DagConfig::default());
        let genesis = Vertex::genesis(1337, 0);
        dag.init_genesis(genesis).unwrap();
        dag
    }
    
    #[test]
    fn test_random_selection() {
        let dag = setup_dag();
        let selector = TipSelector::new(TipSelectionStrategy::Random);
        
        let result = selector.select(&dag);
        
        assert!(!result.parents.is_empty());
        assert_eq!(result.height, 1);
    }
    
    #[test]
    fn test_weighted_selection() {
        let dag = setup_dag();
        let selector = TipSelector::new(TipSelectionStrategy::WeightedRandom);
        
        let result = selector.select(&dag);
        
        assert!(!result.parents.is_empty());
    }
    
    #[test]
    fn test_parent_validation() {
        let dag = setup_dag();
        let tips = dag.get_tips();
        
        // Valid parents
        assert!(validate_parents(&dag, &tips).is_ok());
        
        // Empty parents
        assert!(matches!(
            validate_parents(&dag, &[]),
            Err(TipValidationError::NoParents)
        ));
        
        // Non-existent parent
        assert!(matches!(
            validate_parents(&dag, &[Hash::new([0xFF; 32])]),
            Err(TipValidationError::ParentNotFound(_))
        ));
    }
}
