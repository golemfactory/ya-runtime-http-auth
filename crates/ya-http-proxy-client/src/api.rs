use crate::{web::WebClient, Result};
use ya_http_proxy_model::{
    CreateService, CreateUser, GlobalStats, Service, User, UserEndpointStats, UserStats,
};

#[derive(Clone)]
pub struct ManagementApi {
    client: WebClient,
}

impl ManagementApi {
    pub fn new(client: WebClient) -> Self {
        Self { client }
    }

    // Service management

    pub async fn get_services(&self) -> Result<Vec<Service>> {
        self.client.get("services").await
    }

    pub async fn create_service(&self, cs: &CreateService) -> Result<Service> {
        self.client.post("services", cs).await
    }

    pub async fn get_service(&self, service_name: &str) -> Result<Service> {
        let url = format!("services/{}", service_name);
        self.client.get(&url).await
    }

    pub async fn delete_service(&self, service_name: &str) -> Result<()> {
        let url = format!("services/{}", service_name);
        self.client.delete(&url).await
    }

    // User management per service

    pub async fn get_users(&self, service_name: &str) -> Result<Vec<User>> {
        let url = format!("services/{}/users", service_name);
        self.client.get(&url).await
    }

    pub async fn create_user(&self, service_name: &str, cu: &CreateUser) -> Result<User> {
        let url = format!("services/{}/users", service_name);
        self.client.post(&url, cu).await
    }

    pub async fn get_user(&self, service_name: &str, username: &str) -> Result<User> {
        let url = format!("services/{}/users/{}", service_name, username);
        self.client.get(&url).await
    }

    pub async fn delete_user(&self, service_name: &str, username: &str) -> Result<()> {
        let url = format!("services/{}/users/{}", service_name, username);
        self.client.delete(&url).await
    }

    // User statistics

    pub async fn get_user_stats(&self, service_name: &str, username: &str) -> Result<UserStats> {
        let url = format!("services/{}/users/{}/stats", service_name, username);
        self.client.get(&url).await
    }

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

    // Global statistics

    pub async fn get_global_stats(&self) -> Result<GlobalStats> {
        self.client.get("stats").await
    }
}
