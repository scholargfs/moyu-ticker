//! 当日分时:腾讯 web.ifzq.gtimg.cn 接口(UTF-8 JSON)。
//! 每个分时点字符串形如 "0930 1199.00 969 116183100.00" = 时间 价 累计量(手) 累计额(元)。
//! 均价线 = 累计额 / (累计量 * 100)。

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MinutePoint {
    pub time: String, // "09:30"
    pub price: f64,
    pub avg: f64, // 当日均价
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MinuteData {
    pub code: String,
    pub date: String,
    pub prev_close: Option<f64>, // 昨收,用于分时图基准线
    pub points: Vec<MinutePoint>,
}

const ENDPOINT: &str = "https://web.ifzq.gtimg.cn/appstock/app/minute/query?code=";

pub fn fetch_minute(code: &str) -> Result<MinuteData, String> {
    let url = format!("{}{}", ENDPOINT, code);
    let resp = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?
        .get(&url)
        .header("Referer", "https://finance.qq.com")
        .send()
        .map_err(|e| e.to_string())?;
    let text = resp.text().map_err(|e| e.to_string())?;
    parse_minute(&text, code).ok_or_else(|| "分时数据解析失败".to_string())
}

/// 解析分时 JSON。容错:任一层缺失返回 None。
pub fn parse_minute(json: &str, code: &str) -> Option<MinuteData> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let node = v.get("data")?.get(code)?;
    let inner = node.get("data")?;
    let date = inner
        .get("date")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();
    let arr = inner.get("data")?.as_array()?;

    let prev_close = node
        .get("qt")
        .and_then(|q| q.get(code))
        .and_then(|q| q.as_array())
        .and_then(|a| a.get(4))
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let points = arr
        .iter()
        .filter_map(|item| parse_point(item.as_str()?))
        .collect();

    Some(MinuteData {
        code: code.to_string(),
        date,
        prev_close,
        points,
    })
}

fn parse_point(s: &str) -> Option<MinutePoint> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let raw_time = parts[0]; // "0930"
    if raw_time.len() < 4 {
        return None;
    }
    let time = format!("{}:{}", &raw_time[..2], &raw_time[2..4]);
    let price: f64 = parts[1].parse().ok()?;
    let volume: f64 = parts[2].parse().unwrap_or(0.0);
    let amount: f64 = parts[3].parse().unwrap_or(0.0);
    let avg = if volume > 0.0 {
        amount / (volume * 100.0)
    } else {
        price
    };
    Some(MinutePoint {
        time,
        price,
        avg: (avg * 1000.0).round() / 1000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "code": 0, "msg": "",
      "data": {
        "sh600519": {
          "qt": { "sh600519": ["1","贵州茅台","600519","1185.56","1190.00","1199.00"] },
          "data": {
            "date": "20260626",
            "data": [
              "0930 1199.00 969 116183100.00",
              "0931 1193.85 2240 268166759.81",
              "0932 1191.00 3320 397112727.32"
            ]
          }
        }
      }
    }"#;

    #[test]
    fn parse_points_and_meta() {
        let md = parse_minute(SAMPLE, "sh600519").expect("should parse");
        assert_eq!(md.code, "sh600519");
        assert_eq!(md.date, "20260626");
        assert_eq!(md.prev_close, Some(1190.00));
        assert_eq!(md.points.len(), 3);
        assert_eq!(md.points[0].time, "09:30");
        assert_eq!(md.points[0].price, 1199.00);
        // 均价 = 116183100 / (969*100) ≈ 1198.999
        assert!((md.points[0].avg - 1198.999).abs() < 0.01, "avg={}", md.points[0].avg);
    }

    #[test]
    fn missing_code_returns_none() {
        assert!(parse_minute(SAMPLE, "sh000001").is_none());
    }

    #[test]
    fn invalid_json_returns_none() {
        assert!(parse_minute("not json", "sh600519").is_none());
    }

    #[test]
    fn skips_short_point() {
        let json = r#"{"data":{"x":{"data":{"date":"20260626","data":["0930 10.00 5 5000.00","bad"]}}}}"#;
        let md = parse_minute(json, "x").unwrap();
        assert_eq!(md.points.len(), 1);
        assert_eq!(md.prev_close, None);
    }
}
