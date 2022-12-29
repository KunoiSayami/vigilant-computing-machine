pub trait FromQueryString: for<'de> Deserialize<'de> {
    fn from_query(data: &str) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        serde_teamspeak_querystring::from_str(data)
            .map_err(|e| anyhow::anyhow!("Got parser error: {:?}, original => {:?}", e, data))
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
        type Error = QueryError;

        fn try_from(value: &str) -> Result<Self, Self::Error> {
            let (_, line) = value
                .split_once("error ")
                .ok_or_else(|| anyhow!("Split error: {}", value))?;
            serde_teamspeak_querystring::from_str(line)
                .map_err(|e| QueryError::parse_error(format!("Parse {:?} error: {:?}", line, e)))
        }
    }
}

pub mod connect_info {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Deserialize)]
    pub struct ConnectInfo {
        ip: String,
        port: u16,
    }

    impl ConnectInfo {
        pub fn ip(&self) -> &str {
            &self.ip
        }
        pub fn port(&self) -> u16 {
            self.port
        }
    }

    impl FromQueryString for ConnectInfo {}
}

mod status_result {
    use crate::datastructures::QueryStatus;
    use anyhow::Error;
    use std::fmt::{Display, Formatter};

    pub type QueryResult<T> = Result<T, QueryError>;

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
        pub fn database_id_error() -> Self {
            Self {
                code: -3,
                message: "Can't get self database_id".to_string(),
            }
        }
        pub fn status_not_found() -> Self {
            Self {
                code: -4,
                message: "Status line not found".to_string(),
            }
        }
        pub fn split_error(value: &str) -> Self {
            Self {
                code: -5,
                message: format!("Split error {}", value),
            }
        }
        pub fn parse_error(message: String) -> Self {
            Self { code: -6, message }
        }
        pub fn result_not_found(payload: &str) -> Self {
            Self {
                code: -7,
                message: format!("Result not found: {:?}", payload),
            }
        }
        pub fn query_error(payload: &str) -> Self {
            Self {
                code: -8,
                message: format!("Query client error: {:?}", payload),
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

    impl From<Error> for QueryError {
        fn from(s: Error) -> Self {
            Self {
                code: -114514,
                message: s.to_string(),
            }
        }
    }
}

mod client_variable {

    use super::{from_str, FromQueryString};
    use crate::datastructures::ClientEdit;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct ClientVariable {
        #[serde(deserialize_with = "from_str", rename = "clid")]
        client_id: i64,
        #[serde(rename = "client_description")]
        description: String,
    }

    impl ClientVariable {
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
        pub fn description(&self) -> &str {
            &self.description
        }

        pub fn into_edit(self, client_database_id: i64) -> ClientEdit {
            ClientEdit::new(client_database_id, self.description)
        }
    }

    impl FromQueryString for ClientVariable {}
}

mod client_edit {
    use serde_derive::Serialize;

    #[derive(Clone, Debug, Default, Serialize)]
    pub struct ClientEdit {
        #[serde(rename = "cldbid")]
        client_database_id: i64,
        #[serde(rename = "client_description")]
        description: String,
    }

    impl ClientEdit {
        pub fn new(client_database_id: i64, description: String) -> Self {
            Self {
                client_database_id,
                description,
                ..Default::default()
            }
        }
        pub fn client_database_id(&self) -> i64 {
            self.client_database_id
        }
        pub fn description(&self) -> &str {
            &self.description
        }
    }
}

pub use channel::Channel;
pub use client::Client;
pub use client_edit::ClientEdit;
pub use client_variable::ClientVariable;
pub use connect_info::ConnectInfo;
pub use create_channel::CreateChannel;
pub use query_status::QueryStatus;
use serde::Deserialize;
pub use status_result::{QueryError, QueryResult};
pub use whoami::WhoAmI;
