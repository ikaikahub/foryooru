use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("An API key is needed for Gelbooru")]
    MissingApiKey,
    #[error("Request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("The site answered with status {0}")]
    Status(u16),
    #[error("{site}: {message}")]
    Rejected {
        site: &'static str,
        status: u16,
        message: String,
    },
    #[error("Exports only work with Gelbooru")]
    NeedsGelbooru,
    #[error("Add your Gelbooru API key and login cookie in Settings to sync favourites")]
    NeedsGelbooruLogin,
    #[error("Gelbooru didn't accept the favourite, check the login cookie in Settings")]
    GelbooruFavouriteRefused,
    #[cfg(desktop)]
    #[error("Couldn't open the Gelbooru login: {0}")]
    LoginWindow(String),
    #[error("Add your Danbooru username and API key to sync favourites")]
    NeedsDanbooruLogin,
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unexpected data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("That post isn't cached")]
    PostNotFound,
    #[error("An import is already running")]
    ImportRunning,
    #[error("{0}")]
    InvalidImportFile(String),
    #[cfg(desktop)]
    #[error("Can't save to that location")]
    UnsupportedSaveLocation,
    #[cfg(desktop)]
    #[error("Can't open that file")]
    UnsupportedFile,
    #[cfg(desktop)]
    #[error("Couldn't read the image: {0}")]
    Image(#[from] image::ImageError),
    #[cfg(desktop)]
    #[error("Couldn't use the clipboard: {0}")]
    Clipboard(String),
    #[cfg(target_os = "linux")]
    #[error("Couldn't start the video server: {0}")]
    VideoServer(String),
    #[cfg(target_os = "android")]
    #[error("Couldn't save to the gallery: {0}")]
    Gallery(#[from] tauri::plugin::mobile::PluginInvokeError),
    #[error("Browser storage error: {0}")]
    Storage(String),
    #[error("The engine is still starting")]
    NotStarted,
    #[error("There's no table called {0}")]
    UnknownTable(String),
    #[error("Couldn't read the backup file: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("That file isn't a Foryooru backup")]
    NotABackup,
}

impl Error {
    fn kind(&self) -> &'static str {
        match self {
            Error::MissingApiKey => "missingApiKey",
            Error::Http(_) => "http",
            Error::Status(_) => "status",
            Error::Rejected { .. } => "rejected",
            Error::NeedsGelbooru => "needsGelbooru",
            Error::NeedsGelbooruLogin => "needsGelbooruLogin",
            Error::GelbooruFavouriteRefused => "gelbooruFavouriteRefused",
            #[cfg(desktop)]
            Error::LoginWindow(_) => "loginWindow",
            Error::NeedsDanbooruLogin => "needsDanbooruLogin",
            Error::Database(_) => "database",
            Error::Io(_) => "io",
            Error::Json(_) => "json",
            Error::PostNotFound => "postNotFound",
            Error::ImportRunning => "importRunning",
            Error::InvalidImportFile(_) => "invalidImportFile",
            #[cfg(desktop)]
            Error::UnsupportedSaveLocation => "unsupportedSaveLocation",
            #[cfg(desktop)]
            Error::UnsupportedFile => "unsupportedFile",
            #[cfg(desktop)]
            Error::Image(_) => "image",
            #[cfg(desktop)]
            Error::Clipboard(_) => "clipboard",
            #[cfg(target_os = "linux")]
            Error::VideoServer(_) => "videoServer",
            #[cfg(target_os = "android")]
            Error::Gallery(_) => "gallery",
            Error::Storage(_) => "storage",
            Error::NotStarted => "notStarted",
            Error::UnknownTable(_) => "unknownTable",
            Error::Archive(_) => "archive",
            Error::NotABackup => "notABackup",
        }
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
