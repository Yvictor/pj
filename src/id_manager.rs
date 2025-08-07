use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::info;

pub struct ConnectionIdManager {
    counter: AtomicU64,
    last_reset_time: Mutex<Instant>,
    last_reset_count: AtomicU64,
    reset_interval: Option<Duration>,
    reset_threshold: Option<u64>,
}

impl ConnectionIdManager {
    pub fn new(reset_interval: Option<Duration>, reset_threshold: Option<u64>) -> Self {
        Self {
            counter: AtomicU64::new(0),
            last_reset_time: Mutex::new(Instant::now()),
            last_reset_count: AtomicU64::new(0),
            reset_interval,
            reset_threshold,
        }
    }

    pub fn next_id(&self) -> u64 {
        // Check if we need to reset before incrementing
        let current_count = self.counter.load(Ordering::Relaxed);
        if self.should_reset(current_count) {
            self.reset(current_count);
            // After reset, counter is 0, so fetch_add returns 0 and sets it to 1
            return self.counter.fetch_add(1, Ordering::Relaxed);
        }
        
        // Normal case: increment and return the old value
        self.counter.fetch_add(1, Ordering::Relaxed)
    }

    fn should_reset(&self, current_count: u64) -> bool {
        if let Some(threshold) = self.reset_threshold {
            if current_count >= threshold {
                return true;
            }
        }
        
        if let Some(interval) = self.reset_interval {
            let last_reset = *self.last_reset_time.lock().unwrap();
            if Instant::now().duration_since(last_reset) >= interval {
                return true;
            }
        }
        
        false
    }

    fn reset(&self, last_id: u64) {
        let now = Instant::now();
        let elapsed = {
            let last_reset = *self.last_reset_time.lock().unwrap();
            now.duration_since(last_reset)
        };
        
        let reset_count = self.last_reset_count.fetch_add(1, Ordering::Relaxed) + 1;
        let by_count = self.reset_threshold.map_or(false, |t| last_id >= t);
        let by_time = self.reset_interval.map_or(false, |i| elapsed >= i);
        
        info!(
            "Connection ID reset #{}: {} (last_id: {}, elapsed: {:.2}s)",
            reset_count,
            match (by_count, by_time) {
                (true, false) => "count threshold reached",
                (false, true) => "time interval elapsed",
                (true, true) => "both count and time triggered",
                _ => "unknown trigger",
            },
            last_id,
            elapsed.as_secs_f64()
        );
        
        self.counter.store(0, Ordering::Relaxed);
        *self.last_reset_time.lock().unwrap() = now;
    }
}

pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return Err("Empty duration string".to_string());
    }
    
    let mut total_seconds = 0u64;
    let mut current_num = String::new();
    
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else {
            if current_num.is_empty() {
                return Err(format!("Invalid duration format: missing number before '{}'", ch));
            }
            
            let num: u64 = current_num.parse()
                .map_err(|_| format!("Invalid number: {}", current_num))?;
            
            let multiplier = match ch {
                'd' => 86400,
                'h' => 3600,
                'm' => 60,
                's' => 1,
                _ => return Err(format!("Invalid time unit: '{}'", ch)),
            };
            
            total_seconds += num * multiplier;
            current_num.clear();
        }
    }
    
    if !current_num.is_empty() {
        return Err("Duration must include a unit (d/h/m/s)".to_string());
    }
    
    if total_seconds == 0 {
        return Err("Duration must be greater than 0".to_string());
    }
    
    Ok(Duration::from_secs(total_seconds))
}

pub fn parse_count(s: &str) -> Result<u64, String> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return Err("Empty count string".to_string());
    }
    
    let (num_str, unit) = if s.ends_with('k') {
        (&s[..s.len()-1], 1_000)
    } else if s.ends_with('m') {
        (&s[..s.len()-1], 1_000_000)
    } else if s.ends_with('g') {
        (&s[..s.len()-1], 1_000_000_000)
    } else {
        (s.as_str(), 1)
    };
    
    let num: u64 = num_str.parse()
        .map_err(|_| format!("Invalid number: {}", num_str))?;
    
    if num == 0 {
        return Err("Count must be greater than 0".to_string());
    }
    
    num.checked_mul(unit)
        .ok_or_else(|| "Count value too large".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_single_unit() {
        assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
        assert_eq!(parse_duration("24h").unwrap(), Duration::from_secs(86400));
        assert_eq!(parse_duration("60m").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("3600s").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(parse_duration("1d12h").unwrap(), Duration::from_secs(129600));
        assert_eq!(parse_duration("1h30m").unwrap(), Duration::from_secs(5400));
        assert_eq!(parse_duration("2h30m45s").unwrap(), Duration::from_secs(9045));
    }

    #[test]
    fn test_parse_duration_errors() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("10").is_err());
        assert!(parse_duration("10x").is_err());
        assert!(parse_duration("h10").is_err());
    }

    #[test]
    fn test_parse_count_plain() {
        assert_eq!(parse_count("1000").unwrap(), 1000);
        assert_eq!(parse_count("999999").unwrap(), 999999);
    }

    #[test]
    fn test_parse_count_with_units() {
        assert_eq!(parse_count("1k").unwrap(), 1_000);
        assert_eq!(parse_count("100k").unwrap(), 100_000);
        assert_eq!(parse_count("1m").unwrap(), 1_000_000);
        assert_eq!(parse_count("10m").unwrap(), 10_000_000);
        assert_eq!(parse_count("1g").unwrap(), 1_000_000_000);
    }

    #[test]
    fn test_parse_count_errors() {
        assert!(parse_count("").is_err());
        assert!(parse_count("abc").is_err());
        assert!(parse_count("0").is_err());
        assert!(parse_count("0k").is_err());
        assert!(parse_count("-100").is_err());
    }

    #[test]
    fn test_id_manager_no_reset() {
        let manager = ConnectionIdManager::new(None, None);
        assert_eq!(manager.next_id(), 0);
        assert_eq!(manager.next_id(), 1);
        assert_eq!(manager.next_id(), 2);
    }

    #[test]
    fn test_id_manager_count_reset() {
        let manager = ConnectionIdManager::new(None, Some(3));
        assert_eq!(manager.next_id(), 0);
        assert_eq!(manager.next_id(), 1);
        assert_eq!(manager.next_id(), 2);
        assert_eq!(manager.next_id(), 0); // ID 3 triggers reset, returns 0
        assert_eq!(manager.next_id(), 1);
    }

    #[test]
    fn test_id_manager_time_reset() {
        let manager = ConnectionIdManager::new(Some(Duration::from_millis(100)), None);
        assert_eq!(manager.next_id(), 0);
        assert_eq!(manager.next_id(), 1);
        
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(manager.next_id(), 0); // Time elapsed triggers reset, returns 0
        assert_eq!(manager.next_id(), 1);
    }
}