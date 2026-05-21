pub struct Config {
    pub database_url: String,
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub data_dir: String,
    pub port: u16,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_redirect_uri: String,
    pub frontend_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL required"),
            openai_api_key: std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required"),
            openai_base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            data_dir: std::env::var("COWIKI_DATA_DIR").unwrap_or_else(|_| "./data".into()),
            port: std::env::var("COWIKI_PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .unwrap_or(3000),
            github_client_id: std::env::var("GITHUB_CLIENT_ID")
                .unwrap_or_default(),
            github_client_secret: std::env::var("GITHUB_CLIENT_SECRET")
                .unwrap_or_default(),
            github_redirect_uri: std::env::var("GITHUB_REDIRECT_URI")
                .unwrap_or_else(|_| "http://localhost:3000/api/auth/github/callback".into()),
            frontend_url: std::env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "http://localhost:5173".into()),
        }
    }
}
