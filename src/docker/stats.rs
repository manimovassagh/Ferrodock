//! Live resource usage of a running container.

use futures_util::{Stream, StreamExt};

use bollard::models::ContainerStatsResponse;
use bollard::query_parameters::StatsOptions;

use super::{DockerClient, describe};

/// One reading of a container's resource usage. Byte counters are totals
/// since the container started.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatsSample {
    /// 100% = one full CPU core.
    pub cpu_percent: f64,
    pub memory_used: u64,
    pub memory_limit: u64,
    pub net_rx: u64,
    pub net_tx: u64,
    pub block_read: u64,
    pub block_write: u64,
}

impl From<&ContainerStatsResponse> for StatsSample {
    /// Same maths as `docker stats`.
    fn from(s: &ContainerStatsResponse) -> Self {
        let cpu = s.cpu_stats.as_ref();
        let pre = s.precpu_stats.as_ref();
        let total = |c: Option<&bollard::models::ContainerCpuStats>| {
            c.and_then(|c| c.cpu_usage.as_ref())
                .and_then(|u| u.total_usage)
                .unwrap_or(0)
        };
        let system = |c: Option<&bollard::models::ContainerCpuStats>| {
            c.and_then(|c| c.system_cpu_usage).unwrap_or(0)
        };
        let cpu_delta = total(cpu).saturating_sub(total(pre)) as f64;
        let system_delta = system(cpu).saturating_sub(system(pre)) as f64;
        let cpus = cpu.and_then(|c| c.online_cpus).unwrap_or(1).max(1) as f64;
        let cpu_percent = if cpu_delta > 0.0 && system_delta > 0.0 {
            cpu_delta / system_delta * cpus * 100.0
        } else {
            0.0
        };

        // Page cache isn't really "used"; leave it out like the CLI does
        // (`inactive_file` on cgroup v2, `cache` on v1).
        let memory = s.memory_stats.as_ref();
        let cache = memory
            .and_then(|m| m.stats.as_ref())
            .and_then(|st| st.get("inactive_file").or_else(|| st.get("cache")).copied())
            .unwrap_or(0);
        let memory_used = memory
            .and_then(|m| m.usage)
            .unwrap_or(0)
            .saturating_sub(cache);
        let memory_limit = memory.and_then(|m| m.limit).unwrap_or(0);

        let (net_rx, net_tx) = s
            .networks
            .iter()
            .flat_map(|nets| nets.values())
            .fold((0, 0), |(rx, tx), n| {
                (rx + n.rx_bytes.unwrap_or(0), tx + n.tx_bytes.unwrap_or(0))
            });

        let block = |op: &str| {
            s.blkio_stats
                .as_ref()
                .and_then(|b| b.io_service_bytes_recursive.as_ref())
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|e| e.op.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(op)))
                        .filter_map(|e| e.value)
                        .sum()
                })
                .unwrap_or(0)
        };

        Self {
            cpu_percent,
            memory_used,
            memory_limit,
            net_rx,
            net_tx,
            block_read: block("read"),
            block_write: block("write"),
        }
    }
}

impl DockerClient {
    /// One reading. Docker samples twice, about a second apart, so the CPU
    /// figure is meaningful.
    pub async fn stats_once(&self, id: &str) -> Result<StatsSample, String> {
        let opts = StatsOptions {
            stream: false,
            one_shot: false,
        };
        match self.docker.stats(id, Some(opts)).next().await {
            Some(reading) => reading.map(|s| StatsSample::from(&s)).map_err(describe),
            None => Err("no reading".into()),
        }
    }

    /// Streams a reading about once a second until the container stops.
    pub fn stats(&self, id: &str) -> impl Stream<Item = Result<StatsSample, String>> + use<> {
        let opts = StatsOptions {
            stream: true,
            one_shot: false,
        };
        self.docker
            .stats(id, Some(opts))
            .map(|reading| reading.map(|s| StatsSample::from(&s)).map_err(describe))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::{
        ContainerBlkioStatEntry, ContainerBlkioStats, ContainerCpuStats, ContainerCpuUsage,
        ContainerMemoryStats,
    };
    use std::collections::HashMap;

    fn cpu(total: u64, system: u64) -> ContainerCpuStats {
        ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(total),
                ..Default::default()
            }),
            system_cpu_usage: Some(system),
            online_cpus: Some(4),
            ..Default::default()
        }
    }

    #[test]
    fn computes_like_docker_stats() {
        let response = ContainerStatsResponse {
            precpu_stats: Some(cpu(1_000, 10_000)),
            cpu_stats: Some(cpu(1_500, 12_000)),
            memory_stats: Some(ContainerMemoryStats {
                usage: Some(300),
                limit: Some(1_000),
                stats: Some(HashMap::from([("inactive_file".to_string(), 100)])),
                ..Default::default()
            }),
            blkio_stats: Some(ContainerBlkioStats {
                io_service_bytes_recursive: Some(vec![
                    ContainerBlkioStatEntry {
                        op: Some("read".into()),
                        value: Some(7),
                        ..Default::default()
                    },
                    ContainerBlkioStatEntry {
                        op: Some("Write".into()),
                        value: Some(9),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let sample = StatsSample::from(&response);
        // 500 / 2000 of the machine, times 4 CPUs.
        assert!((sample.cpu_percent - 100.0).abs() < 1e-9);
        assert_eq!(sample.memory_used, 200);
        assert_eq!(sample.memory_limit, 1_000);
        assert_eq!((sample.block_read, sample.block_write), (7, 9));
    }

    #[test]
    fn first_reading_has_no_cpu_delta() {
        let sample = StatsSample::from(&ContainerStatsResponse::default());
        assert_eq!(sample, StatsSample::default());
    }
}
