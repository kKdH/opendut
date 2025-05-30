use std::cmp::Ordering;

use crate::resource::persistence::error::PersistenceResult;
use opendut_types::resources::Id;
use redb::{AccessGuard, ReadableTable, TableError, TypeName};
use uuid::Uuid;

#[cfg(feature="postgres")] pub mod database;
pub(crate) mod error;
pub(crate) mod persistable;
#[cfg(feature="postgres")]
pub(crate) mod query;

pub type Memory<'transaction> = Db<'transaction>;

pub enum Db<'transaction> {
    Read(&'transaction redb::ReadTransaction),
    ReadWrite(&'transaction mut redb::WriteTransaction),
}
impl Db<'_> {
    pub(super) fn read_table(&self, table: TableDefinition) -> PersistenceResult<Option<ReadTable>> {
        let open_result = match self {
            Db::Read(transaction) => transaction.open_table(table).map(ReadTable::Read),
            Db::ReadWrite(transaction) => transaction.open_table(table).map(ReadTable::ReadWrite),
        };

        match open_result { //The ReadTransaction does not automatically create the table and rather returns a TableDoesNotExist error
            Ok(table) => Ok(Some(table)),
            Err(cause) => match cause {
                TableError::TableDoesNotExist(_) => Ok(None),
                _ => Err(redb::Error::from(cause))?,
            }
        }
    }

    pub(crate) fn read_write_table(&self, table: TableDefinition) -> PersistenceResult<ReadWriteTable> {
        match self {
            Db::Read(_) => unimplemented!("Called `.read_write_table()` on a Db::Read() variant."),
            Db::ReadWrite(transaction) => Ok(transaction.open_table(table)?),
        }
    }
}

pub(super) enum ReadTable<'transaction> {
    Read(redb::ReadOnlyTable<Key, Value>),
    ReadWrite(redb::Table<'transaction, Key, Value>),
}
impl ReadTable<'_> {
    pub(crate) fn get(&self, key: &Key) -> redb::Result<Option<AccessGuard<Value>>> {
        match self {
            ReadTable::Read(table) => table.get(key),
            ReadTable::ReadWrite(table) => table.get(key),
        }
    }
    pub(crate) fn iter(&self) -> redb::Result<redb::Range<Key, Value>> {
        match self {
            ReadTable::Read(table) => table.iter(),
            ReadTable::ReadWrite(table) => table.iter(),
        }
    }
}
pub(super) type ReadWriteTable<'a> = redb::Table<'a, Key, Value>;

#[derive(Debug)]
pub struct Key { pub id: Id }

impl redb::Key for Key {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        let data1 = Uuid::from_slice(data1)
            .expect("A UUID which was previously saved to bytes should be loadable from bytes.");
        let data2 = Uuid::from_slice(data2)
            .expect("A UUID which was previously saved to bytes should be loadable from bytes.");

        data1.cmp(&data2)
    }
}
impl redb::Value for Key {
    type SelfType<'a> = Self
    where
        Self: 'a;

    type AsBytes<'a> = [u8; 16]
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(16)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a
    {
        let uuid = Uuid::from_slice(data)
            .expect("A PersistenceId which was previously saved to bytes should be loadable from bytes.");
        Key { id: Id::from(uuid) }
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b
    {
        let Key { id } = value;
        *id.value().as_bytes()
    }

    fn type_name() -> TypeName {
        TypeName::new("opendut_persistence_id")
    }
}
impl From<Id> for Key {
    fn from(value: Id) -> Self {
        Key { id: value }
    }
}

pub(super) type Value = Vec<u8>;
pub(super) type TableDefinition<'a> = redb::TableDefinition<'a, Key, Value>;


#[cfg(any(
    test, //normal unit tests
    all(test, doc) //doc tests, but excluded `cargo doc` (because some dependencies are only dev-dependencies)
))]
pub mod testing {
    use crate::resource::manager::{ResourceManager, ResourceManagerRef};
    use crate::resource::storage::{DatabaseConnectInfo, PersistenceOptions};
    use assert_fs::fixture::PathChild;

    /// Spawns a Postgres Container and returns a ResourceManager for testing.
    /// ```no_run
    /// # use std::any::Any;
    /// # use opendut_carl::resource::persistence;
    ///
    /// #[tokio::test]
    /// async fn test() {
    ///     let mut db = persistence::testing::spawn_and_connect_resource_manager().await?;
    ///
    ///     do_something_with_resource_manager(db.resource_manager);
    /// }
    ///
    /// # fn do_something_with_resource_manager(resource_manager: impl Any) {}
    /// ```
    pub async fn spawn_and_connect_resource_manager() -> anyhow::Result<PostgresResources> {
        let (connect_info, temp_dir) = spawn().await?;

        let persistence_options = PersistenceOptions::Enabled {
            database_connect_info: connect_info,
        };
        let resource_manager = ResourceManager::create(&persistence_options).await?;

        Ok(PostgresResources { resource_manager, temp_dir })
    }
    pub struct PostgresResources {
        pub resource_manager: ResourceManagerRef,
        #[allow(unused)] //carried along to extend its lifetime until the end of the test (database file is deleted when variable is dropped)
        temp_dir: assert_fs::TempDir,
    }

    async fn spawn() -> anyhow::Result<(DatabaseConnectInfo, assert_fs::TempDir)> {
        let temp_dir = assert_fs::TempDir::new()?;
        let file = temp_dir.child("opendut.db");

        let connect_info = DatabaseConnectInfo {
            file: file.to_path_buf(),

            #[cfg(feature="postgres")]
            url: url::Url::parse("postgres://localhost")?, //to make things compile
            #[cfg(feature="postgres")]
            username: String::from("postgres"),
            #[cfg(feature="postgres")]
            password: crate::resource::storage::Password::new_static("postgres"),
        };

        Ok((connect_info, temp_dir))
    }
}
