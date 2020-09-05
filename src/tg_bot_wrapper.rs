extern crate reqwest;

use reqwest::{Client, Url};
use serde::Serialize;

pub struct TgBot {
    token: String,
}

impl TgBot {
    pub fn new(token: String) -> Self {
        TgBot { token }
    }

    fn get_client(&self) -> Client {
        Client::new()
    }

    async fn api_request<S: AsRef<str>, Q: Serialize + ?Sized>(
        &self,
        endpoint: S,
        params: &Q,
    ) -> reqwest::Result<reqwest::Response> {
        Ok(self
            .get_client()
            .post(self.get_url().join(endpoint.as_ref()).unwrap())
            .query(params)
            .send()
            .await?)
    }

    pub async fn send_message<S: AsRef<str>>(
        &self,
        chat_id: u64,
        text: S,
    ) -> reqwest::Result<reqwest::Response> {
        Ok(self
            .api_request(
                "sendMessage",
                &[
                    ("chat_id", chat_id.to_string().as_str()),
                    ("text", text.as_ref()),
                ],
            )
            .await?)
    }

    pub fn get_url(&self) -> Url {
        Url::parse(format!("https://api.telegram.org/bot{}/", self.token).as_str()).unwrap()
    }
}
