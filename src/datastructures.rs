pub trait FromQueryString: for<'de> Deserialize<'de> {
    fn from_query(data: &str) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        serde_teamspeak_querystring::from_str(data)
            .map_err(|e| anyhow::anyhow!("Got parser error: {:?}", e))
    }
}

fn from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    T::from_str(&s).map_err(serde::de::Error::custom)
}

pub mod whoami {
    use super::{from_str, FromQueryString};
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct WhoAmI {
        #[serde(deserialize_with = "from_str", rename = "clid")]
        client_id: i64,
        #[serde(deserialize_with = "from_str", rename = "cid")]
        channel_id: i64,
    }

    impl WhoAmI {
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
        pub fn channel_id(&self) -> i64 {
            self.channel_id
        }
    }

    impl FromQueryString for WhoAmI {}
}

pub mod create_channel {
    use super::{from_str, FromQueryString};
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct CreateChannel {
        #[serde(deserialize_with = "from_str")]
        cid: i64,
    }

    impl CreateChannel {
        pub fn cid(&self) -> i64 {
            self.cid
        }
    }

    impl FromQueryString for CreateChannel {}
}

pub mod channel {
    use super::{from_str, FromQueryString};
    use serde_derive::Deserialize;

    #[allow(dead_code)]
    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Channel {
        #[serde(deserialize_with = "from_str")]
        cid: i64,
        #[serde(deserialize_with = "from_str")]
        pid: i64,
        #[serde(deserialize_with = "from_str")]
        channel_order: i64,
        channel_name: String,
        #[serde(deserialize_with = "from_str")]
        total_clients: i64,
    }

    #[allow(dead_code)]
    impl Channel {
        pub fn cid(&self) -> i64 {
            self.cid
        }
        pub fn pid(&self) -> i64 {
            self.pid
        }
        pub fn channel_order(&self) -> i64 {
            self.channel_order
        }
        pub fn channel_name(&self) -> &str {
            &self.channel_name
        }
        pub fn total_clients(&self) -> i64 {
            self.total_clients
        }
    }

    impl FromQueryString for Channel {}
}

pub mod client {
    use super::from_str;
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[allow(dead_code)]
    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Client {
        #[serde(deserialize_with = "from_str")]
        clid: i64,
        #[serde(deserialize_with = "from_str")]
        cid: i64,
        #[serde(deserialize_with = "from_str")]
        client_database_id: i64,
        #[serde(deserialize_with = "from_str")]
        client_type: i64,
        //client_unique_identifier: String,
        client_nickname: String,
    }

    #[allow(dead_code)]
    impl Client {
        pub fn client_id(&self) -> i64 {
            self.clid
        }
        pub fn channel_id(&self) -> i64 {
            self.cid
        }
        pub fn client_database_id(&self) -> i64 {
            self.client_database_id
        }
        pub fn client_type(&self) -> i64 {
            self.client_type
        }
        pub fn client_unique_identifier(&self) -> String {
            format!("{}", self.client_database_id)
        }
        pub fn client_nickname(&self) -> &str {
            &self.client_nickname
        }
    }

    impl FromQueryString for Client {}

    #[cfg(test)]
    mod test {
        use crate::datastructures::client::Client;
        use crate::datastructures::FromQueryString;

        const TEST_STRING: &str = "clid=8 cid=1 client_database_id=1 client_nickname=serveradmin client_type=1 client_unique_identifier=serveradmin";

        #[test]
        fn test() {
            let result = Client::from_query(TEST_STRING).unwrap();
            assert_eq!(result.client_id(), 8);
            assert_eq!(result.channel_id(), 1);
            assert_eq!(result.client_database_id(), 1);
            assert_eq!(result.client_nickname(), "serveradmin".to_string());
            assert_eq!(result.client_type(), 1);
            assert_eq!(result.client_unique_identifier(), "serveradmin".to_string());
        }
    }
}

pub mod query_status {
    use crate::datastructures::{QueryError, QueryResult};
    use anyhow::anyhow;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Deserialize)]
    pub struct WebQueryStatus {
        code: i32,
        message: String,
    }

    impl WebQueryStatus {
        pub fn into_status(self) -> QueryStatus {
            QueryStatus {
                id: self.code,
                msg: self.message,
            }
        }
    }

    impl From<WebQueryStatus> for QueryStatus {
        fn from(status: WebQueryStatus) -> Self {
            status.into_status()
        }
    }

    #[allow(dead_code)]
    #[derive(Clone, Debug, Deserialize)]
    pub struct QueryStatus {
        id: i32,
        msg: String,
    }

    impl Default for QueryStatus {
        fn default() -> Self {
            Self {
                id: 0,
                msg: "ok".to_string(),
            }
        }
    }

    impl QueryStatus {
        pub fn id(&self) -> i32 {
            self.id
        }
        pub fn msg(&self) -> &String {
            &self.msg
        }

        pub fn into_err(self) -> QueryError {
            QueryError::from(self)
        }

        pub fn into_result<T>(self, ret: T) -> QueryResult<T> {
            if self.id == 0 {
                return Ok(ret);
            }
            Err(self.into_err())
        }
    }

    impl TryFrom<&str> for QueryStatus {
        type Error = anyhow::Error;

        fn try_from(value: &str) -> Result<Self, Self::Error> {
            let (_, line) = value
                .split_once("error ")
                .ok_or_else(|| anyhow!("Split error: {}", value))?;
            serde_teamspeak_querystring::from_str(line)
                .map_err(|e| anyhow!("Got error while parse string: {:?} {:?}", line, e))
        }
    }
}

pub mod connect_info {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Deserialize)]
    pub struct ConnnectInfo {
        ip: String,
        port: u16,
    }

    impl ConnnectInfo {
        pub fn ip(&self) -> &str {
            &self.ip
        }
        pub fn port(&self) -> u16 {
            self.port
        }
    }

    impl FromQueryString for ConnnectInfo {}
}

pub mod config {
    use anyhow::anyhow;
    use serde_derive::Deserialize;
    use std::fs::read_to_string;
    use std::path::Path;

    #[derive(Clone, Debug, Deserialize)]
    #[serde(untagged)]
    pub enum Integer {
        Single(i64),
        Multiple(Vec<i64>),
    }

    impl Integer {
        fn to_vec(&self) -> Vec<i64> {
            match self {
                Integer::Single(id) => {
                    vec![*id]
                }
                Integer::Multiple(ids) => ids.clone(),
            }
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Server {
        address: String,
        //port: u16,
        channel: String,
        timeout: Option<u64>,
        password: Option<String>,
        switch_wait: Option<u64>,
    }

    impl Server {
        pub fn address(&self) -> &str {
            &self.address
        }
        /*pub fn port(&self) -> u16 {
            self.port
        }*/
        pub fn channel(&self) -> &str {
            &self.channel
        }

        pub fn timeout(&self) -> u64 {
            self.timeout.map(|x| if x < 3 { 3 } else { x }).unwrap_or(3)
        }
        pub fn password(&self) -> &Option<String> {
            &self.password
        }
        pub fn switch_wait(&self) -> u64 {
            self.switch_wait
                .map(|x| if x <= 500 { 500 } else { x })
                .unwrap_or(500)
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Monitor {
        #[serde(rename = "web")]
        web_enabled: bool,
        username: String,
        backend: String,
        interval: Option<u64>,
    }

    impl Monitor {
        pub fn web_enabled(&self) -> bool {
            self.web_enabled
        }
        pub fn username(&self) -> &str {
            &self.username
        }
        pub fn backend(&self) -> &str {
            &self.backend
        }
        pub fn interval(&self) -> u64 {
            self.interval
                .map(|x| if x == 0 { 1 } else { x })
                .unwrap_or(1)
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Config {
        api_key: String,
        monitor_id: Integer,
        need_disconnect: Option<bool>,
        monitor: Monitor,
        server: Server,
    }

    impl Config {
        pub fn api_key(&self) -> &str {
            &self.api_key
        }
        pub fn need_disconnect(&self) -> bool {
            self.need_disconnect.unwrap_or_default()
        }
        pub fn monitor_id(&self) -> Vec<i64> {
            self.monitor_id.to_vec()
        }
        pub fn monitor(&self) -> &Monitor {
            &self.monitor
        }
        pub fn server(&self) -> &Server {
            &self.server
        }
    }

    impl TryFrom<&Path> for Config {
        type Error = anyhow::Error;

        fn try_from(path: &Path) -> Result<Self, Self::Error> {
            let content = read_to_string(path).map_err(|e| anyhow!("Read error: {:?}", e))?;

            let result: Self =
                toml::from_str(&content).map_err(|e| anyhow!("Deserialize toml error: {:?}", e))?;
            Ok(result)
        }
    }
}

mod status_result {
    use crate::datastructures::QueryStatus;
    use anyhow::Error;
    use std::fmt::{Display, Formatter};

    pub type QueryResult<T> = std::result::Result<T, QueryError>;

    #[derive(Clone, Default, Debug)]
    pub struct QueryError {
        code: i32,
        message: String,
    }

    impl QueryError {
        pub fn static_empty_response() -> Self {
            Self {
                code: -1,
                message: "Expect result but none found.".to_string(),
            }
        }
        pub fn channel_not_found() -> Self {
            Self {
                code: -2,
                message: "Channel not found".to_string(),
            }
        }
        pub fn database_id_error() -> Self {
            Self {
                code: -3,
                message: "Can't get self database_id".to_string(),
            }
        }
        pub fn code(&self) -> i32 {
            self.code
        }
    }

    impl Display for QueryError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}({})", self.message, self.code)
        }
    }

    impl std::error::Error for QueryError {}

    impl From<QueryStatus> for QueryError {
        fn from(status: QueryStatus) -> Self {
            Self {
                code: status.id(),
                message: status.msg().clone(),
            }
        }
    }

    impl From<anyhow::Error> for QueryError {
        fn from(s: Error) -> Self {
            Self {
                code: -2,
                message: s.to_string(),
            }
        }
    }
}

pub use channel::Channel;
pub use client::Client;
pub use config::Config;
pub use connect_info::ConnnectInfo;
pub use create_channel::CreateChannel;
pub use query_status::{QueryStatus, WebQueryStatus};
use serde::Deserialize;
pub use status_result::{QueryError, QueryResult};
pub use whoami::WhoAmI;
