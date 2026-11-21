use std::path::{Path, PathBuf};

pub type CoreResult<T> = Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no se pudo acceder a {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("formato VDF no válido en {0}")]
    Vdf(String),
    #[error("JSON no válido en {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("no se pudo leer el archivo comprimido {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("clase Java no válida: {0}")]
    Class(String),
    #[error("archivo de mapa no válido: {0}")]
    Map(String),
    #[error("no encontrado: {0}")]
    NotFound(String),
    #[error("{0}")]
    Backup(String),
}

impl Error {
    pub fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
        move |source| Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}
