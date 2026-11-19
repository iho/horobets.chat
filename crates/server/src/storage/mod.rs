use std::path::PathBuf;
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, key: &str, data: &[u8]) -> anyhow::Result<()>;
    async fn get(&self, key: &str) -> anyhow::Result<Vec<u8>>;
}
pub struct LocalFsStore {
    root: PathBuf,
}
impl LocalFsStore {
    pub fn new(root: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    fn path(&self, key: &str) -> anyhow::Result<PathBuf> {
        uuid::Uuid::parse_str(key)?;
        Ok(self.root.join(key))
    }
}
#[async_trait::async_trait]
impl BlobStore for LocalFsStore {
    async fn put(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
        let path = self.path(key)?;
        let tmp = self.root.join(format!("{}.tmp", crate::id()));
        tokio::fs::write(&tmp, data).await?;
        tokio::fs::rename(tmp, path).await?;
        Ok(())
    }
    async fn get(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        Ok(tokio::fs::read(self.path(key)?).await?)
    }
}

/// S3 and Cloudflare R2, configured through the standard AWS_* variables.
#[cfg(feature = "s3")]
pub struct S3Store(object_store::aws::AmazonS3);
#[cfg(feature = "s3")]
impl S3Store {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self(
            object_store::aws::AmazonS3Builder::from_env().build()?,
        ))
    }
}
#[cfg(feature = "s3")]
#[async_trait::async_trait]
impl BlobStore for S3Store {
    async fn put(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
        use object_store::ObjectStoreExt;
        uuid::Uuid::parse_str(key)?;
        self.0
            .put(&object_store::path::Path::from(key), data.to_vec().into())
            .await?;
        Ok(())
    }
    async fn get(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        use object_store::ObjectStoreExt;
        uuid::Uuid::parse_str(key)?;
        Ok(self
            .0
            .get(&object_store::path::Path::from(key))
            .await?
            .bytes()
            .await?
            .to_vec())
    }
}
