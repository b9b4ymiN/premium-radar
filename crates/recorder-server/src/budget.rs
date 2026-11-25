//! Per-minute request budget — defence in depth คู่กับ throttle 1.1s/request
//! ที่ transport มีอยู่แล้ว: นับทุก request ที่ *server* ออกเอง (ผ่าน
//! `HttpFetch`) และปฏิเสธ (ไม่ส่งออกจากเครื่อง) เมื่อเกินโควตานาทีนั้น

use std::time::{Duration, Instant};

use radar_core::webull::rest::{FetchError, HttpFetch};
use serde_json::Value;

/// ตัวนับ fixed-window รายนาที (inject เวลาได้เพื่อ test)
pub struct MinuteBudget {
    max: u32,
    window_start: Instant,
    count: u32,
}

impl MinuteBudget {
    pub fn new(max: u32) -> Self {
        Self {
            max,
            window_start: Instant::now(),
            count: 0,
        }
    }

    /// true = ผ่าน (นับด้วย) · false = เกินโควตาของนาทีนี้ (หน้าต่างใหม่
    /// เริ่มนับใหม่เองเมื่อครบ 60s)
    pub fn permit_at(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_start) >= Duration::from_secs(60) {
            self.window_start = now;
            self.count = 0;
        }
        if self.count >= self.max {
            false
        } else {
            self.count += 1;
            true
        }
    }
}

/// ครอบ client จริงด้วย budget — request ที่ถูกปฏิเสธไม่เคยไปถึง network
pub struct BudgetedClient<F> {
    pub inner: F,
    pub budget: MinuteBudget,
}

impl<F> BudgetedClient<F> {
    pub fn new(inner: F, max_per_minute: u32) -> Self {
        Self {
            inner,
            budget: MinuteBudget::new(max_per_minute),
        }
    }
}

impl<F: HttpFetch> HttpFetch for BudgetedClient<F> {
    fn get(&mut self, path: &str, params: &[(String, String)]) -> Result<(u16, Value), FetchError> {
        if !self.budget.permit_at(Instant::now()) {
            return Err(FetchError::Network(
                "per-minute request budget หมดแล้ว —  request นี้ถูกทิ้งก่อนออกจากเครื่อง".into(),
            ));
        }
        self.inner.get(path, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// fetcher ปลอมนับจำนวนครั้ง — ไม่มี network
    struct CountingFetch;
    impl HttpFetch for CountingFetch {
        fn get(
            &mut self,
            _path: &str,
            _params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            Ok((200, serde_json::json!({ "data": [] })))
        }
    }

    #[test]
    fn budget_denies_after_max_and_resets_next_minute() {
        let t0 = Instant::now();
        let mut b = MinuteBudget::new(2);
        assert!(b.permit_at(t0));
        assert!(b.permit_at(t0 + Duration::from_secs(10)));
        assert!(!b.permit_at(t0 + Duration::from_secs(20))); // เกิน 2 ในนาทีเดียวกัน
        // ขึ้นหน้าต่างใหม่ 61s หลังเริ่ม — นับใหม่
        assert!(b.permit_at(t0 + Duration::from_secs(61)));
    }

    #[test]
    fn budgeted_client_drops_request_before_network() {
        let mut c = BudgetedClient::new(CountingFetch, 1);
        let p: Vec<(String, String)> = Vec::new();
        assert!(c.get("/x", &p).is_ok());
        let denied = c.get("/x", &p);
        match denied {
            Err(FetchError::Network(msg)) => assert!(msg.contains("budget")),
            other => panic!("expected budget denial, got {other:?}"),
        }
    }
}
