use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Router {
    pub instances: HashMap<String, String>,
    pub guild_map: Arc<RwLock<HashMap<String, String>>>,
}

#[derive(Debug)]
pub enum RouteTarget {
    Instance(String, String),
}

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("instance '{0}' not found")]
    InstanceNotFound(String),
    #[error("guild '{0}' not mapped to any instance")]
    GuildNotFound(String),
    #[error("no instance or guild_id specified")]
    NoTarget,
}

impl Router {
    pub fn new(instances: HashMap<String, String>) -> Self {
        Self {
            instances,
            guild_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn resolve(
        &self,
        instance: Option<&str>,
        guild_id: Option<&str>,
    ) -> Result<RouteTarget, RouteError> {
        if let Some(name) = instance {
            let url = self.instances.get(name)
                .ok_or_else(|| RouteError::InstanceNotFound(name.to_string()))?;
            return Ok(RouteTarget::Instance(name.to_string(), url.clone()));
        }
        if let Some(gid) = guild_id {
            let map = self.guild_map.read().await;
            if let Some(name) = map.get(gid) {
                let url = self.instances.get(name)
                    .ok_or_else(|| RouteError::InstanceNotFound(name.to_string()))?;
                return Ok(RouteTarget::Instance(name.to_string(), url.clone()));
            }
            return Err(RouteError::GuildNotFound(gid.to_string()));
        }
        Err(RouteError::NoTarget)
    }

    pub async fn update_guild_map(&self, instance_name: &str, guild_ids: Vec<String>) {
        let mut map = self.guild_map.write().await;
        map.retain(|_, v| v != instance_name);
        for gid in guild_ids {
            map.insert(gid, instance_name.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_router() -> Router {
        let mut instances = HashMap::new();
        instances.insert("examplebot".to_string(), "http://examplebot:9090".to_string());
        instances.insert("secondbot".to_string(), "http://secondbot:9090".to_string());
        Router::new(instances)
    }

    #[tokio::test]
    async fn resolve_explicit_instance() {
        let router = test_router();
        let result = router.resolve(Some("secondbot"), None).await.unwrap();
        match result {
            RouteTarget::Instance(name, url) => {
                assert_eq!(name, "secondbot");
                assert_eq!(url, "http://secondbot:9090");
            }
        }
    }

    #[tokio::test]
    async fn resolve_unknown_instance_fails() {
        let router = test_router();
        let result = router.resolve(Some("unknown"), None).await;
        assert!(matches!(result, Err(RouteError::InstanceNotFound(_))));
    }

    #[tokio::test]
    async fn resolve_by_guild_id() {
        let router = test_router();
        router.update_guild_map("secondbot", vec!["123456789012345678".to_string()]).await;
        let result = router.resolve(None, Some("123456789012345678")).await.unwrap();
        match result {
            RouteTarget::Instance(name, _) => assert_eq!(name, "secondbot"),
        }
    }

    #[tokio::test]
    async fn resolve_unknown_guild_fails() {
        let router = test_router();
        let result = router.resolve(None, Some("99999")).await;
        assert!(matches!(result, Err(RouteError::GuildNotFound(_))));
    }

    #[tokio::test]
    async fn resolve_no_target_fails() {
        let router = test_router();
        let result = router.resolve(None, None).await;
        assert!(matches!(result, Err(RouteError::NoTarget)));
    }

    #[tokio::test]
    async fn explicit_instance_overrides_guild() {
        let router = test_router();
        router.update_guild_map("secondbot", vec!["123".to_string()]).await;
        let result = router.resolve(Some("examplebot"), Some("123")).await.unwrap();
        match result {
            RouteTarget::Instance(name, _) => assert_eq!(name, "examplebot"),
        }
    }

    #[tokio::test]
    async fn guild_map_updates_replace_old_entries() {
        let router = test_router();
        router.update_guild_map("secondbot", vec!["111".to_string(), "222".to_string()]).await;
        router.update_guild_map("secondbot", vec!["333".to_string()]).await;
        assert!(router.resolve(None, Some("111")).await.is_err());
        assert!(router.resolve(None, Some("222")).await.is_err());
        assert!(router.resolve(None, Some("333")).await.is_ok());
    }
}
