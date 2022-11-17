use crate::model::{
    CreateService, CreateUser, GlobalStats, Service, User, UserEndpointStats, UserStats,
};
use crate::{web::WebClient, Result};

/// Handle to a proxy api.
#[derive(Clone)]
pub struct ManagementApi {
    client: WebClient,
}

impl ManagementApi {
    /// Creates connection to proxy management api.
    pub fn try_default() -> Result<Self> {
        Ok(Self::new(WebClient::try_default()?))
    }

    /// Creates connection to proxy managmet api at given url.
    pub fn try_from_url(url: &str) -> Result<Self> {
        Ok(Self::new(WebClient::new(url)?))
    }

    fn new(client: WebClient) -> Self {
        Self { client }
    }

    /// Lists available services.
    pub async fn get_services(&self) -> Result<Vec<Service>> {
        self.client.get("services").await
    }

    /// Create new service from spec.
    pub async fn create_service(&self, cs: &CreateService) -> Result<Service> {
        self.client.post("services", cs).await
    }

    /// Gets service by name.
    pub async fn get_service(&self, service_name: &str) -> Result<Service> {
        let url = format!("services/{}", service_name);
        self.client.get(&url).await
    }

    /// Drops service.
    pub async fn delete_service(&self, service_name: &str) -> Result<()> {
        let url = format!("services/{}", service_name);
        self.client.delete(&url).await
    }

    /// User management per service
    pub async fn get_users(&self, service_name: &str) -> Result<Vec<User>> {
        let url = format!("services/{}/users", service_name);
        self.client.get(&url).await
    }

    /// Add user to service
    pub async fn create_user(&self, service_name: &str, cu: &CreateUser) -> Result<User> {
        let url = format!("services/{}/users", service_name);
        self.client.post(&url, cu).await
    }

    /// Get user info for service.
    pub async fn get_user(&self, service_name: &str, username: &str) -> Result<User> {
        let url = format!("services/{}/users/{}", service_name, username);
        self.client.get(&url).await
    }

    /// Removes giver user from given server.
    pub async fn delete_user(&self, service_name: &str, username: &str) -> Result<()> {
        let url = format!("services/{}/users/{}", service_name, username);
        self.client.delete(&url).await
    }

    /// User statistics
    pub async fn get_user_stats(&self, service_name: &str, username: &str) -> Result<UserStats> {
        let url = format!("services/{}/users/{}/stats", service_name, username);
        self.client.get(&url).await
    }

    /// List user endpoints stats.
    pub async fn get_endpoint_user_stats(
        &self,
        service_name: &str,
        username: &str,
    ) -> Result<UserEndpointStats> {
        let url = format!(
            "services/{}/users/{}/endpoints/stats",
            service_name, username
        );
        self.client.get(&url).await
    }

    /// Global statistics.
    pub async fn get_global_stats(&self) -> Result<GlobalStats> {
        self.client.get("stats").await
    }
}
