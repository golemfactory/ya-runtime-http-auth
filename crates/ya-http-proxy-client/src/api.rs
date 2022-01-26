use crate::model::{CreateService, CreateUser, Service, User, UserEndpointStats, UserStats};
use crate::{web::WebClient, Result};

#[derive(Clone)]
pub struct ManagementApi {
    client: WebClient,
}

impl ManagementApi {
    pub fn new(client: &WebClient) -> Self {
        let client = client.clone();
        Self { client }
    }

    // Service management

    pub async fn get_services(&self) -> Result<Vec<Service>> {
        self.client.get("services").await
    }

    pub async fn create_service(&self, cs: &CreateService) -> Result<CreateService> {
        self.client.post("services", cs).await
    }

    pub async fn get_service(&self, service_name: &str) -> Result<Service> {
        let url = url_format!("services/{service_name}", service_name);
        self.client.get(&url).await
    }

    pub async fn delete_service(&self, service_name: &str) -> Result<()> {
        let url = url_format!("services/{service_name}", service_name);
        self.client.delete(&url).await
    }

    // User management per service

    pub async fn get_users(&self, service_name: &str) -> Result<Vec<User>> {
        let url = url_format!("services/{service_name}/users", service_name);
        self.client.get(&url).await
    }

    pub async fn create_user(&self, service_name: &str, cu: &CreateUser) -> Result<CreateUser> {
        let url = url_format!("services/{service_name}/users", service_name);
        self.client.post(&url, cu).await
    }

    pub async fn get_user(&self, service_name: &str, user_name: &str) -> Result<User> {
        let url = url_format!(
            "services/{service_name}/users/{user_name}",
            service_name,
            user_name
        );
        self.client.get(&url).await
    }

    pub async fn delete_user(&self, service_name: &str, user_name: &str) -> Result<()> {
        let url = url_format!(
            "services/{service_name}/users/{user_name}",
            service_name,
            user_name
        );
        self.client.delete(&url).await
    }

    // User statistics

    pub async fn get_user_stats(&self, service_name: &str, user_name: &str) -> Result<UserStats> {
        let url = url_format!(
            "services/{service_name}/users/{user_name}/stats",
            service_name,
            user_name
        );
        self.client.get(&url).await
    }

    pub async fn get_endpoint_user_stats(
        &self,
        service_name: &str,
        user_name: &str,
    ) -> Result<UserEndpointStats> {
        let url = url_format!(
            "services/{service_name}/users/{user_name}/endpoints/stats",
            service_name,
            user_name
        );
        self.client.get(&url).await
    }
}
