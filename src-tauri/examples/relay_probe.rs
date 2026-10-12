//! 独立探针：用与 Safe Switch relay 完全相同的 reqwest 配置请求 DeepSeek，
//! 用于隔离"reqwest 本身行为" vs "转发代码细节"导致的 400。
use reqwest::{header, Client};

#[tokio::main]
async fn main() {
    let client = Client::builder()
        .http1_only()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .unwrap();

    let url = "https://api.deepseek.com/chat/completions";
    let body = r#"{"model":"deepseek-chat","messages":[{"role":"user","content":"hi"}],"max_tokens":5,"stream":false}"#;

    let resp = client
        .post(url)
        .header(
            header::AUTHORIZATION,
            "Bearer sk-4b5ba59d04ef4bed8c757613abf72234",
        )
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::USER_AGENT, "Python-urllib/3.13")
        .header(header::ACCEPT, "*/*")
        .body(body)
        .send()
        .await
        .expect("send failed");

    println!("status: {}", resp.status());
    let text = resp.text().await.unwrap();
    println!("body: {}", text.chars().take(300).collect::<String>());
}
