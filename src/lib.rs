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

    #[error("Scottatura [PARSE_003]: Sub-serves are not supported in v1.")]
    SubServeNotSupported,

    #[error("Scottatura [PARSE_004]: Multi-column strainers are not supported in v1.")]
    MultiColumnStrainerNotSupported,

    #[error("Scottatura [TIPO_003]: Unknown type `{type_name}`. Supported types: INTEGER / INT / BIGINT, REAL / FLOAT, TEXT, BLOB.")]
    UnknownType { type_name: String },

    #[error("Scottatura [RICETTA_006]: Column `id` is reserved for the Al Dente Key; remove it from the column list.")]
    ReservedIdColumn,

    #[error("Scottatura [PARSE_005]: LEFT / RIGHT / FULL TOSS is not supported in v1; use TOSS (inner equi-join).")]
    OuterTossNotSupported,

    #[error("Scottatura [PARSE_006]: Only equi-joins (ON a.col = b.col) are supported.")]
    NonEquiJoinNotSupported,

    #[error("Scottatura [PARSE_009]: SKIP without ONLY is not supported.")]
    SkipWithoutOnly,

    #[error("Scottatura [PARSE_010]: HAVING is not supported in v1.")]
    HavingNotSupported,

    #[error("Scottatura [PARSE_011]: At most one TOSS per serve is supported in v1.")]
    MultipleTossNotSupported,

    #[error("Scottatura [RICETTA_009]: Value count ({got}) does not match column count ({expected}).")]
    ValueCountMismatch { got: usize, expected: usize },

    #[error("Scottatura [PARSE_007]: SEASON requires a STRAINED BY clause.")]
    SeasonRequiresFilter,

    #[error("Scottatura [PARSE_008]: DISCARD FROM requires a STRAINED BY clause.")]
    DiscardRequiresFilter,

    #[error("Scottatura [PARSE_002]: Statement type not supported.")]
    StatementNotSupported,

    #[error("Scottatura [RICETTA_005]: Cannot update the Al Dente Key (`id`) column.")]
    AlDenteKeyNotUpdatable,

    #[error("Scottatura [EXEC_001]: `{clause}` is not supported yet — planned for a later phase.")]
    ClauseNotYetSupported { clause: String },

    #[error("Scottatura [EXEC_002]: Simmer transactions (BEGIN/FINISH/SPILL) are not executable yet; only single-statement autocommit is supported.")]
    TransactionNotSupported,

    #[error("Scottatura [EXEC_003]: Strainer indexes (STRAIN / REMOVE STRAINER) are not supported yet.")]
    StrainerNotSupported,
}
