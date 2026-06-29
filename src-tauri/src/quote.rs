//! 实时行情:腾讯 qt.gtimg.cn 接口。GBK 编码,需解码后按 `~` 解析。
//! 涨跌额/涨跌幅由 现价与昨收 自行计算,避免依赖易变的字段下标。

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Quote {
    pub code: String,       // 带前缀,如 sh600519
    pub name: String,       // 名称
    pub price: f64,         // 现价
    pub prev_close: f64,    // 昨收
    pub change: f64,        // 涨跌额
    pub change_pct: f64,    // 涨跌幅 %
}

const ENDPOINT: &str = "https://qt.gtimg.cn/q=";

/// 拼批量请求 URL,如 q=sh000001,sz000002
pub fn build_url(codes: &[String]) -> String {
    format!("{}{}", ENDPOINT, codes.join(","))
}

/// 抓取并解析一组代码的实时行情。网络/解码失败返回 Err,单只解析失败则被跳过。
pub fn fetch_quotes(codes: &[String]) -> Result<Vec<Quote>, String> {
    if codes.is_empty() {
        return Ok(vec![]);
    }
    let url = build_url(codes);
    let resp = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?
        .get(&url)
        .header("Referer", "https://finance.qq.com")
        .send()
        .map_err(|e| e.to_string())?;
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    // 腾讯接口返回 GBK 编码
    let (text, _, _) = encoding_rs::GBK.decode(&bytes);
    Ok(parse_response(&text))
}

/// 解析整段响应(已解码为 UTF-8)。每行形如:
/// v_sh600519="1~贵州茅台~600519~1185.56~1190.00~...";
pub fn parse_response(text: &str) -> Vec<Quote> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Quote> {
    let line = line.trim();
    if !line.starts_with("v_") {
        return None;
    }
    let eq = line.find('=')?;
    let code = line[2..eq].to_string(); // 去掉 v_ 前缀 -> sh600519
    // 取等号后引号内的内容
    let rest = &line[eq + 1..];
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')? + start;
    let payload = &rest[start..end];

    let f: Vec<&str> = payload.split('~').collect();
    // 至少需要 名称(1) 现价(3) 昨收(4)
    if f.len() < 5 {
        return None;
    }
    let name = f[1].trim().to_string();
    let price: f64 = f[3].trim().parse().ok()?;
    let prev_close: f64 = f[4].trim().parse().ok()?;
    if name.is_empty() {
        return None;
    }
    let change = price - prev_close;
    let change_pct = if prev_close.abs() > f64::EPSILON {
        change / prev_close * 100.0
    } else {
        0.0
    };
    Some(Quote {
        code,
        name,
        price,
        prev_close,
        change: (change * 1000.0).round() / 1000.0,
        change_pct: (change_pct * 100.0).round() / 100.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_joins_codes() {
        let codes = vec!["sh000001".to_string(), "sz000002".to_string()];
        assert_eq!(build_url(&codes), "https://qt.gtimg.cn/q=sh000001,sz000002");
    }

    #[test]
    fn parse_normal_line() {
        // 截取自真实响应的字段布局(名称已为解码后 UTF-8)
        let sample = "v_sh600519=\"1~贵州茅台~600519~1185.56~1190.00~1199.00~660108~0~0~0.00\";";
        let quotes = parse_response(sample);
        assert_eq!(quotes.len(), 1);
        let q = &quotes[0];
        assert_eq!(q.code, "sh600519");
        assert_eq!(q.name, "贵州茅台");
        assert_eq!(q.price, 1185.56);
        assert_eq!(q.prev_close, 1190.00);
        assert!((q.change - (-4.44)).abs() < 1e-6, "change={}", q.change);
        assert!((q.change_pct - (-0.37)).abs() < 1e-6, "pct={}", q.change_pct);
    }

    #[test]
    fn parse_multiple_lines() {
        let sample = "v_sh000001=\"1~上证指数~000001~4027.26~4120.28~4098.69\";\n\
                      v_sz399001=\"51~深证成指~399001~13000.00~13130.00~13100.00\";";
        let quotes = parse_response(sample);
        assert_eq!(quotes.len(), 2);
        assert_eq!(quotes[0].code, "sh000001");
        assert_eq!(quotes[1].code, "sz399001");
    }

    #[test]
    fn skip_malformed_line() {
        // 字段不足 / 价格非数字 的行应被跳过,不影响整体
        let sample = "v_sh000001=\"1~上证指数~000001~4027.26~4120.28\";\n\
                      garbage line\n\
                      v_sz000002=\"51~万科A~000002~bad~10.00\";";
        let quotes = parse_response(sample);
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].code, "sh000001");
    }

    #[test]
    fn red_green_sign() {
        // 红涨绿跌由前端按 change 正负决定;此处确认正负号正确
        let up = "v_sh600000=\"1~浦发银行~600000~11.00~10.00~10.50\";";
        let q = &parse_response(up)[0];
        assert!(q.change > 0.0);
    }
}
