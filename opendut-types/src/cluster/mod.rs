use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fmt::Formatter;
use std::ops::Not;
use std::str::FromStr;
use uuid::Uuid;

pub use assignment::*;

use crate::peer::PeerId;
use crate::topology::DeviceId;

mod assignment;
pub mod state;


#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClusterId(pub Uuid);

impl ClusterId {
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }
}

impl From<Uuid> for ClusterId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

#[derive(thiserror::Error, Clone, Debug)]
#[error("Illegal ClusterId: {value}")]
pub struct IllegalClusterId {
    pub value: String,
}

impl TryFrom<&str> for ClusterId {

    type Error = IllegalClusterId;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| IllegalClusterId { value: String::from(value) })
    }
}

impl fmt::Display for ClusterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ClusterId {
    type Err = IllegalClusterId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterName(pub(crate) String);

impl ClusterName {

    pub const MIN_LENGTH: usize = 4;
    pub const MAX_LENGTH: usize = 64;

    pub fn value(self) -> String {
        self.0
    }
}

#[derive(thiserror::Error, Clone, Debug)]
pub enum IllegalClusterName {
    #[error("Cluster name '{value}' is too short. Expected at least {expected} characters, got {actual}.")]
    TooShort { value: String, expected: usize, actual: usize },
    #[error("Cluster name '{value}' is too long. Expected at most {expected} characters, got {actual}.")]
    TooLong { value: String, expected: usize, actual: usize },
    #[error("Cluster name '{value}' contains invalid characters.")]
    InvalidCharacter { value: String },
    #[error("Cluster name '{value}' contains invalid start or end characters.")]
    InvalidStartEndCharacter { value: String },
}

impl From<ClusterName> for String {
    fn from(value: ClusterName) -> Self {
        value.0
    }
}

impl TryFrom<String> for ClusterName {

    type Error = IllegalClusterName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let length = value.len();
        if length < Self::MIN_LENGTH {
            Err(IllegalClusterName::TooShort {
                value,
                expected: Self::MIN_LENGTH,
                actual: length,
            })
        }
        else if length > Self::MAX_LENGTH {
            Err(IllegalClusterName::TooLong {
                value,
                expected: Self::MAX_LENGTH,
                actual: length,
            })
        }
        else if crate::util::invalid_start_and_end_of_a_name(&value) {
            Err(IllegalClusterName::InvalidStartEndCharacter { value })
        }
        else if value.chars().any(|c| crate::util::valid_characters_in_name(&c).not()) {
            Err(IllegalClusterName::InvalidCharacter {
                value
            })
        }
        else {
            Ok(Self(value))
        }
    }
}

impl TryFrom<&str> for ClusterName {

    type Error = IllegalClusterName;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ClusterName::try_from(value.to_owned())
    }
}

impl fmt::Display for ClusterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterConfiguration {
    pub id: ClusterId,
    pub name: ClusterName,
    pub leader: PeerId,
    pub devices: HashSet<DeviceId>,
}

#[derive(thiserror::Error, Clone, Debug)]
pub enum IllegalClusterConfiguration {
    #[error("{0}")]
    InvalidName(IllegalClusterName),
    #[error("At least two devices are required to form a cluster.")]
    TooFewDevices,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterDeployment {
    pub id: ClusterId,
}


pub struct ClusterDisplay {
    cluster_name: Option<ClusterName>,
    cluster_id: ClusterId,
}
impl ClusterDisplay {
    pub fn new(cluster_name: &Option<ClusterName>, cluster_id: &ClusterId) -> Self {
        Self {
            cluster_name: cluster_name.to_owned(),
            cluster_id: cluster_id.to_owned(),
        }
    }
}
impl fmt::Display for ClusterDisplay {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let ClusterDisplay { cluster_name, cluster_id } = self;

        let cluster_name = cluster_name.as_ref()
            .map(|cluster_name| format!("'{cluster_name}' "))
            .unwrap_or_default();

        write!(f, "{cluster_name}<{cluster_id}>")
    }
}


#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[test]
    fn A_PeerName_should_contain_valid_characters() -> Result<()> {
        let peer_name = ClusterName::try_from("asd123".to_string()).expect("Failed to create peer name");
        assert_eq!(peer_name.0, "asd123");
        Ok(())
    }

    #[test]
    fn A_PeerName_should_not_start_an_underscore() -> Result<()> {
        let _peer_name = ClusterName::try_from("_asd123".to_string()).is_err();
        Ok(())
    }
}
