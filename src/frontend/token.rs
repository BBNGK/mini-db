use std::collections::HashMap;

pub enum TokenType {
    Unknown,
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

// Struct to take the input string and tokenize it using a Hashmap to store keywords and operations
pub struct Tokenizer {
    input: String,
    position: usize,
    keyword: HashMap<String, TokenType>,
    operation: HashMap<String, TokenType>,
}

impl Token {
    pub fn new(token_type: TokenType, literal: String) -> Token {
        Token {
            token_type,
            literal,
        }
    }
}

impl Tokenizer {
    // Taking the character at each 'position' and delegating it to the appropriate collector.
    // After all inputs are used, Return None.
    fn new(input: String) -> Self {
        let keyword = HashMap::from([
            ("SELECT".to_string(), TokenType::Select),
            ("FROM".to_string(), TokenType::From),
            ("WHERE".to_string(), TokenType::Where),
            ("INSERT".to_string(), TokenType::Insert),
            ("INTO".to_string(), TokenType::Into),
            ("VALUES".to_string(), TokenType::Values),
            ("CREATE".to_string(), TokenType::Create),
            ("TABLE".to_string(), TokenType::Table),
            ("DELETE".to_string(), TokenType::Delete),
            ("UPDATE".to_string(), TokenType::Update),
            ("SET".to_string(), TokenType::Set),
            ("INT".to_string(), TokenType::Int),
            ("AND".to_string(), TokenType::And),
            ("OR".to_string(), TokenType::Or),
        ]);
        let operation = HashMap::from([
            ("=".to_string(), TokenType::Equal),
            ("!=".to_string(), TokenType::NotEqual),
            ("<".to_string(), TokenType::LessThan),
            (">".to_string(), TokenType::GreaterThan),
            ("<=".to_string(), TokenType::LessThanEqual),
            (">=".to_string(), TokenType::GreaterThanEqual),
            ("*".to_string(), TokenType::Asterisk),
            (",".to_string(), TokenType::Comma),
            (";".to_string(), TokenType::Semicolon),
            ("(".to_string(), TokenType::LeftParen),
            (")".to_string(), TokenType::RightParen),
        ]);
        Tokenizer {
            input,
            position: 0,
            keyword,
            operation,
        }
    }
    fn get_next_token(&mut self) -> Option<Token> {
        if self.position >= self.input.len() {
            return None;
        }
        // helpful for reading in larger inputs since Rust is bad with constant-time indexing for strings.
        let current_character = self.input.chars().nth(self.position).unwrap();
        if current_character.is_alphabetic() {
            return self.collect_identifier();
        } else {
            self.position += 1;
            return Some(Token::new(
                TokenType::Unknown,
                current_character.to_string(),
            ));
        }
    }
    // a run of alphabetic characters gets consumed as a token identifier
    // Note: only alphabetic chars are read
    fn collect_identifier(&mut self) -> Option<Token> {
        let start_position = self.position;
        while self.position < self.input.len()
            && self
                .input
                .chars()
                .nth(self.position)
                .unwrap()
                .is_alphabetic()
        {
            self.position += 1;
        }
        Some(Token::new(
            TokenType::Identifier,
            self.input[start_position..self.position].to_string(),
        ))
    }
    // returns all the tokens in the tokenizer
    fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while let Some(token) = self.get_next_token() {
            tokens.push(token);
        }
        tokens
    }
}
