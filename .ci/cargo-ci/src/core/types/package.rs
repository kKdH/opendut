use std::fmt::{Display, Formatter};

/// Package used somewhere in the build process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIter)]
pub enum Package {
    Carl,
    CarlApi,
    Cleo,
    Edgar,
    Lea,
    NetbirdClientApi,
    Types,
    Theo,
    Util,
    Vpn,
    VpnNetbird,
    IntegrationTests,
}

impl Package {
    pub fn ident(&self) -> String {
        match self {
            Package::Carl => "opendut-carl",
            Package::CarlApi => "opendut-carl-api",
            Package::Cleo => "opendut-cleo",
            Package::Edgar => "opendut-edgar",
            Package::Lea => "opendut-lea",
            Package::NetbirdClientApi => "opendut-netbird-client-api",
            Package::Types => "opendut-types",
            Package::Theo => "opendut-theo",
            Package::Util => "opendut-util",
            Package::Vpn => "opendut-vpn",
            Package::VpnNetbird => "opendut-vpn-netbird",
            Package::IntegrationTests => "opendut-integration-tests",
        }.to_string()
    }
}

impl Display for Package {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ident())
    }
}


