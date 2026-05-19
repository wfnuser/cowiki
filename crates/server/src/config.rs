pub struct Config {
    pub database_url: String,
    pub openai_api_key: String,
    pub openai_base_url: String,
    pub data_dir: String,
    pub port: u16,
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
        }
    }
}
