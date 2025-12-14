//! 用户闲置检测模块

use std::time::Duration;
use tracing::debug;

/// 闲置检测器
pub struct IdleDetector {
    /// 闲置阈值（毫秒）
    threshold_ms: u64,
}

impl IdleDetector {
    /// 创建新的闲置检测器
    pub fn new(threshold_ms: u64) -> Self {
        Self { threshold_ms }
    }

    /// 设置闲置阈值
    pub fn set_threshold(&mut self, threshold_ms: u64) {
        self.threshold_ms = threshold_ms;
    }

    /// 获取当前闲置时间（毫秒）
    pub fn get_idle_time_ms(&self) -> u64 {
        match user_idle::UserIdle::get_time() {
            Ok(idle) => idle.as_millis() as u64,
            Err(e) => {
                debug!("Failed to get idle time: {}", e);
                0
            }
        }
    }

    /// 检查用户是否处于闲置状态
    pub fn is_idle(&self) -> bool {
        let idle_time = self.get_idle_time_ms();
        let is_idle = idle_time >= self.threshold_ms;

        if is_idle {
            debug!("User is idle ({}ms >= {}ms threshold)", idle_time, self.threshold_ms);
        }

        is_idle
    }

    /// 获取闲置阈值
    pub fn threshold(&self) -> Duration {
        Duration::from_millis(self.threshold_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_detector_creation() {
        let detector = IdleDetector::new(30000);
        assert_eq!(detector.threshold_ms, 30000);
    }

    #[test]
    fn test_set_threshold() {
        let mut detector = IdleDetector::new(30000);
        detector.set_threshold(60000);
        assert_eq!(detector.threshold_ms, 60000);
    }
}
