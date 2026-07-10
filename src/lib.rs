pub mod parser;
pub mod prep_log;
pub mod sous_chef;
pub mod stove;
pub mod strainer;
pub mod warming_tray;

pub use stove::pot::Pot;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    Null,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColType {
    Integer,
    Real,
    Text,
    Blob,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: ColType,
    pub nullable: bool,
    pub strainer_root: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Recipe {
    pub noodle_name: String,
    pub columns: Vec<ColumnDef>,
}

#[derive(Debug, Default)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub affected_rows: u64,
    pub last_insert_key: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum Scottatura {
    #[error("Scottatura [STOVE_002]: Magic number invalid — `{path}` is not a pot file.")]
    NotAPotFile { path: String },

    #[error("Scottatura [STOVE_001]: Page checksum mismatch on plate {page} — pot may be corrupt.")]
    PageChecksumMismatch { page: u64 },

    #[error("Scottatura [STOVE_004]: I/O error on pot file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Scottatura [RICETTA_001]: Noodle `{name}` does not exist.")]
    NoodleNotFound { name: String },

    #[error("Scottatura [RICETTA_002]: Noodle `{name}` already exists.")]
    NoodleAlreadyExists { name: String },

    #[error("Scottatura [RICETTA_003]: Ingredient `{col}` not found in noodle `{name}`.")]
    ColumnNotFound { col: String, name: String },

    #[error("Scottatura [TIPO_001]: Cannot store {got} value in ingredient `{col}` (expected {expected}).")]
    TypeMismatch { got: String, col: String, expected: String },

    #[error("Scottatura [VINCOLO_002]: Vuoto not allowed in ingredient `{col}` (declared NOT NULL).")]
    NullConstraintViolation { col: String },

    #[error("Scottatura [PARSE_001]: Unexpected token `{token}` at position {pos}.")]
    UnexpectedToken { token: String, pos: usize },

    #[error("Scottatura [PARSE_012]: Identifier `{name}` exceeds maximum length of 128 characters.")]
    IdentifierTooLong { name: String },

    #[error("Scottatura [TIPO_004]: Invalid literal `{literal}` for type {type_name}.")]
    InvalidLiteral { literal: String, type_name: String },
}
