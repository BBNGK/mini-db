pub enum TokenType {
    Identifier,
    Number,
    String,
    Select,
    From,
    Where,
    Insert,
    Into,
    Values,
    Create,
    Table,
    Delete,
    Update,
    Set,
    Int,
    And,
    Or,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanEqual,
    GreaterThanEqual,
    Asterisk,
    Comma,
    Semicolon,
    LeftParen,
    RightParen,
}

pub struct Token {
    pub token_type: TokenType,
    pub literal: String,
}

impl Token {
    pub fn new(token_type: TokenType, literal: String) -> Token {
        Token {
            token_type,
            literal,
        }
    }
}