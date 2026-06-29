//! 轮询:仅在 A 股交易时段(周一至五 09:30–11:30 / 13:00–15:00)抓取,
//! 盘后停止以省电、降暴露。每轮把最新行情 emit 给前端。

use crate::quote;
use chrono::{Datelike, NaiveDateTime, NaiveTime, Weekday};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// 判断给定本地时间是否处于 A 股连续竞价交易时段。
pub fn is_trading(dt: NaiveDateTime) -> bool {
    match dt.weekday() {
        Weekday::Sat | Weekday::Sun => return false,
        _ => {}
    }
    let t = dt.time();
    let morning_open = NaiveTime::from_hms_opt(9, 30, 0).unwrap();
    let morning_close = NaiveTime::from_hms_opt(11, 30, 0).unwrap();
    let afternoon_open = NaiveTime::from_hms_opt(13, 0, 0).unwrap();
    let afternoon_close = NaiveTime::from_hms_opt(15, 0, 0).unwrap();
    (t >= morning_open && t <= morning_close) || (t >= afternoon_open && t <= afternoon_close)
}

/// 轮询间隔:盘中 3s,盘后 60s(只为尽快感知开盘,不抓数据)。
fn interval_secs(trading: bool) -> u64 {
    if trading {
        3
    } else {
        60
    }
}

/// 启动后台轮询线程。watchlist 为共享自选股列表(可在运行时被设置页修改)。
/// 每轮:盘中抓取并 emit "quotes-updated";失败 emit "quotes-stale"(不打断前端)。
pub fn start_polling(app: AppHandle, watchlist: Arc<Mutex<Vec<String>>>) {
    std::thread::spawn(move || loop {
        let now = chrono::Local::now().naive_local();
        let trading = is_trading(now);

        if trading {
            let codes = { watchlist.lock().unwrap().clone() };
            if !codes.is_empty() {
                match quote::fetch_quotes(&codes) {
                    Ok(quotes) => {
                        let _ = app.emit("quotes-updated", &quotes);
                    }
                    Err(e) => {
                        let _ = app.emit("quotes-stale", e);
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(interval_secs(trading)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    #[test]
    fn weekday_morning_session_open() {
        // 2026-06-29 是周一
        assert!(is_trading(dt(2026, 6, 29, 9, 30)));
        assert!(is_trading(dt(2026, 6, 29, 10, 0)));
        assert!(is_trading(dt(2026, 6, 29, 11, 30)));
    }

    #[test]
    fn weekday_afternoon_session() {
        assert!(is_trading(dt(2026, 6, 29, 13, 0)));
        assert!(is_trading(dt(2026, 6, 29, 14, 59)));
        assert!(is_trading(dt(2026, 6, 29, 15, 0)));
    }

    #[test]
    fn lunch_break_and_pre_open_closed() {
        assert!(!is_trading(dt(2026, 6, 29, 9, 29))); // 开盘前
        assert!(!is_trading(dt(2026, 6, 29, 12, 0))); // 午休
        assert!(!is_trading(dt(2026, 6, 29, 15, 1))); // 收盘后
    }

    #[test]
    fn weekend_closed() {
        // 2026-06-27 周六, 06-28 周日
        assert!(!is_trading(dt(2026, 6, 27, 10, 0)));
        assert!(!is_trading(dt(2026, 6, 28, 14, 0)));
    }

    #[test]
    fn interval_switches() {
        assert_eq!(interval_secs(true), 3);
        assert_eq!(interval_secs(false), 60);
    }
}
