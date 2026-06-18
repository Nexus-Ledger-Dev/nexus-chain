//! Metrics and monitoring for DAG consensus

use nexus_primitives::Timestamp;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// DAG performance metrics
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DagMetrics {
    /// Total vertices processed
    pub vertices_processed: u64,
    
    /// Total transactions processed
    pub transactions_processed: u64,
    
    /// Current TPS (transactions per second)
    pub current_tps: f64,
    
    /// Average confirmation latency (ms)
    pub avg_confirmation_latency_ms: f64,
    
    /// Average finality latency (ms)
    pub avg_finality_latency_ms: f64,
    
    /// Current tip count
    pub tip_count: usize,
    
    /// DAG width (average vertices per height)
    pub dag_width: f64,
    
    /// Memory usage (bytes)
    pub memory_usage: u64,
    
    /// Orphan vertices (pending parents)
    pub orphan_count: usize,
}

/// Metrics collector
pub struct MetricsCollector {
    /// Rolling window of transaction counts for TPS calculation
    tx_window: RwLock<VecDeque<(Instant, u64)>>,
    
    /// Confirmation latencies
    confirmation_latencies: RwLock<VecDeque<u64>>,
    
    /// Finality latencies
    finality_latencies: RwLock<VecDeque<u64>>,
    
    /// Window size for rolling averages
    window_size: usize,
    
    /// Accumulated metrics
    accumulated: RwLock<AccumulatedMetrics>,
}

#[derive(Default)]
struct AccumulatedMetrics {
    vertices_processed: u64,
    transactions_processed: u64,
    total_confirmation_latency: u64,
    total_finality_latency: u64,
    confirmation_count: u64,
    finality_count: u64,
}

impl MetricsCollector {
    pub fn new(window_size: usize) -> Self {
        Self {
            tx_window: RwLock::new(VecDeque::with_capacity(window_size)),
            confirmation_latencies: RwLock::new(VecDeque::with_capacity(window_size)),
            finality_latencies: RwLock::new(VecDeque::with_capacity(window_size)),
            window_size,
            accumulated: RwLock::new(AccumulatedMetrics::default()),
        }
    }
    
    /// Record a processed vertex
    pub fn record_vertex(&self, tx_count: usize) {
        let mut acc = self.accumulated.write();
        acc.vertices_processed += 1;
        acc.transactions_processed += tx_count as u64;
        
        // Update TPS window
        let mut window = self.tx_window.write();
        window.push_back((Instant::now(), tx_count as u64));
        
        // Remove old entries (older than 10 seconds)
        let cutoff = Instant::now() - Duration::from_secs(10);
        while window.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
            window.pop_front();
        }
    }
    
    /// Record confirmation latency
    pub fn record_confirmation(&self, latency_ms: u64) {
        let mut latencies = self.confirmation_latencies.write();
        if latencies.len() >= self.window_size {
            latencies.pop_front();
        }
        latencies.push_back(latency_ms);
        
        let mut acc = self.accumulated.write();
        acc.total_confirmation_latency += latency_ms;
        acc.confirmation_count += 1;
    }
    
    /// Record finality latency
    pub fn record_finality(&self, latency_ms: u64) {
        let mut latencies = self.finality_latencies.write();
        if latencies.len() >= self.window_size {
            latencies.pop_front();
        }
        latencies.push_back(latency_ms);
        
        let mut acc = self.accumulated.write();
        acc.total_finality_latency += latency_ms;
        acc.finality_count += 1;
    }
    
    /// Calculate current TPS
    pub fn current_tps(&self) -> f64 {
        let window = self.tx_window.read();
        
        if window.len() < 2 {
            return 0.0;
        }
        
        let total_txs: u64 = window.iter().map(|(_, c)| *c).sum();
        
        if let (Some((start, _)), Some((end, _))) = (window.front(), window.back()) {
            let duration = end.duration_since(*start).as_secs_f64();
            if duration > 0.0 {
                return total_txs as f64 / duration;
            }
        }
        
        0.0
    }
    
    /// Get current metrics snapshot
    pub fn snapshot(&self, tip_count: usize, orphan_count: usize) -> DagMetrics {
        let acc = self.accumulated.read();
        
        let avg_confirmation = if acc.confirmation_count > 0 {
            acc.total_confirmation_latency as f64 / acc.confirmation_count as f64
        } else {
            0.0
        };
        
        let avg_finality = if acc.finality_count > 0 {
            acc.total_finality_latency as f64 / acc.finality_count as f64
        } else {
            0.0
        };
        
        DagMetrics {
            vertices_processed: acc.vertices_processed,
            transactions_processed: acc.transactions_processed,
            current_tps: self.current_tps(),
            avg_confirmation_latency_ms: avg_confirmation,
            avg_finality_latency_ms: avg_finality,
            tip_count,
            dag_width: 0.0, // Would need height distribution
            memory_usage: 0, // Would need actual memory tracking
            orphan_count,
        }
    }
    
    /// Reset metrics
    pub fn reset(&self) {
        *self.accumulated.write() = AccumulatedMetrics::default();
        self.tx_window.write().clear();
        self.confirmation_latencies.write().clear();
        self.finality_latencies.write().clear();
    }
}

/// Throughput analyzer
pub struct ThroughputAnalyzer {
    #[allow(dead_code)]
    sample_period_secs: u64,
    
    /// Historical TPS samples
    tps_history: RwLock<VecDeque<(Timestamp, f64)>>,
    
    /// Maximum history length
    max_history: usize,
}

impl ThroughputAnalyzer {
    pub fn new(sample_period_secs: u64, max_history: usize) -> Self {
        Self {
            sample_period_secs,
            tps_history: RwLock::new(VecDeque::with_capacity(max_history)),
            max_history,
        }
    }
    
    /// Record a TPS sample
    pub fn record_sample(&self, timestamp: Timestamp, tps: f64) {
        let mut history = self.tps_history.write();
        
        if history.len() >= self.max_history {
            history.pop_front();
        }
        
        history.push_back((timestamp, tps));
    }
    
    /// Get average TPS over a time window
    pub fn average_tps(&self, window_secs: u64) -> f64 {
        let history = self.tps_history.read();
        
        if history.is_empty() {
            return 0.0;
        }
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let cutoff = now.saturating_sub(window_secs * 1000);
        
        let relevant: Vec<f64> = history.iter()
            .filter(|(ts, _)| *ts >= cutoff)
            .map(|(_, tps)| *tps)
            .collect();
        
        if relevant.is_empty() {
            0.0
        } else {
            relevant.iter().sum::<f64>() / relevant.len() as f64
        }
    }
    
    /// Get peak TPS
    pub fn peak_tps(&self) -> f64 {
        self.tps_history.read()
            .iter()
            .map(|(_, tps)| *tps)
            .fold(0.0, f64::max)
    }
    
    /// Get TPS percentiles
    pub fn tps_percentiles(&self) -> TpsPercentiles {
        let history = self.tps_history.read();
        
        if history.is_empty() {
            return TpsPercentiles::default();
        }
        
        let mut values: Vec<f64> = history.iter().map(|(_, tps)| *tps).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let len = values.len();
        
        TpsPercentiles {
            p50: values[len / 2],
            p90: values[(len * 90) / 100],
            p95: values[(len * 95) / 100],
            p99: values[(len * 99) / 100],
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TpsPercentiles {
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new(100);
        
        collector.record_vertex(10);
        collector.record_vertex(20);
        collector.record_confirmation(100);
        collector.record_finality(500);
        
        let metrics = collector.snapshot(5, 2);
        
        assert_eq!(metrics.vertices_processed, 2);
        assert_eq!(metrics.transactions_processed, 30);
        assert_eq!(metrics.tip_count, 5);
        assert_eq!(metrics.orphan_count, 2);
    }
    
    #[test]
    fn test_throughput_analyzer() {
        let analyzer = ThroughputAnalyzer::new(1, 100);
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        analyzer.record_sample(now - 1000, 100.0);
        analyzer.record_sample(now - 500, 150.0);
        analyzer.record_sample(now, 200.0);
        
        assert!(analyzer.average_tps(10) > 0.0);
        assert_eq!(analyzer.peak_tps(), 200.0);
    }
}
