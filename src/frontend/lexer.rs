use crate::frontend::token::{Token, TokenType};
use std::collections::HashMap;



// Struct to take the input string and tokenize it using a Hashmap to store keywords and operations
pub struct Lexer {
    input: String, //Raw SQL queries/text that is scanned
    position: usize, // Current index inside the string
    keyword: HashMap<&'static str, TokenType>, // SQL keywords
    operation: HashMap<&'static str, TokenType>, // Operations/ Symbols
}

impl Lexer {
    // Constructor to build lookup tables for keywords and operations
    // Used lookup tables(HashMaps) to help bypass if/else checks
    fn new(input: String) -> Self {
        let keyword = HashMap::from([
            ("SELECT", TokenType::Select),
            ("FROM", TokenType::From),
            ("WHERE", TokenType::Where),
            ("INSERT", TokenType::Insert),
            ("INTO", TokenType::Into),
            ("VALUES", TokenType::Values),
            ("CREATE", TokenType::Create),
            ("TABLE", TokenType::Table),
            ("DELETE", TokenType::Delete),
            ("UPDATE", TokenType::Update),
            ("SET", TokenType::Set),
            ("INT", TokenType::Int),
            ("AND", TokenType::And),
            ("OR", TokenType::Or),
        ]);
        let operation = HashMap::from([
            ("=", TokenType::Equal),
            ("!=", TokenType::NotEqual),
            ("<", TokenType::LessThan),
            (">", TokenType::GreaterThan),
            ("<=", TokenType::LessThanEqual),
            (">=", TokenType::GreaterThanEqual),
            ("*", TokenType::Asterisk),
            (",", TokenType::Comma),
            (";", TokenType::Semicolon),
            ("(", TokenType::LeftParen),
            (")", TokenType::RightParen),
        ]);
        Lexer {
            input,
            position: 0,
            keyword,
            operation,
        }
    }

    // function to return the current char at the position if it exists
    fn current_char(&self) -> Option<char> {
        self.input.chars().nth(self.position)
    }

    // Main function that loops through the hashmap to tokenize keywords and operations:
    // Skips whitespaces, reads one token, and loops until the end
    fn get_next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        let ch= self.current_char()?;

        // Any letters or underscore could likely be a keyword
        if ch.is_ascii_alphabetic() || ch == '_' {
            return Some(self.collect_identifier());
        }

        // Any symbols that could be potential operators, they are classified
        if matches!(ch, '=' | '!'| '<' | '>'| '*'| ','| ';'| '('| ')') {
           return Some(self.collect_operator());
        }

        // for unknown single-char tokens
        self.position += ch.len_utf8();
        Some(Token::new(TokenType::Unknown, ch.to_string()))
    }

    // function to skip any whitespaces
    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current_char() {
            if ch.is_whitespace() {
                self.position += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    // Take any keywords to classify and use keywords in order to look for a match
    fn collect_identifier(&mut self) -> Token {
        let start_position = self.position;

        // loop to read characters that are letters or underscores
        while let Some(ch) = self.current_char() {
            if ch.is_ascii_alphabetic() || ch == '_' {
                self.position += ch.len_utf8();
            } else {
                break;
            }
        }
        
        // Extract any text that was taken in
        let val = self.input[start_position..self.position].to_string();

        // to normalize so that "SELECT" and "select" are not mapped to different keys
        let upper_case = val.to_uppercase();
        
        // to match the keyword in the keywords table
        match self.keyword.get(&upper_case.as_str()) {
            Some(token_type) => Token::new(*token_type, val),
            None => Token::new(TokenType::Identifier, val),
        }
    }

    // function that collects two-char operators
    // uses operations hashmap to find token type
    fn collect_operator(&mut self) -> Token {
        let first = self.current_char().unwrap();
        let mut val = first.to_string();

        // used to move past the first character
        self.position += first.len_utf8();

        // a check to see if the two-char operator exists
        if let Some(next) = self.current_char() {
            let two_ch = format!("{}{}", first, next);
            if self.operation.contains_key(&two_ch.as_str()) {
                self.position += next.len_utf8();
                val = two_ch;
            }
        }

        // looking up the operator in the operation hashmap
        match self.operation.get(val.as_str()) {
            Some(token_type) => Token::new(*token_type, val),
            None => Token::new(TokenType::Identifier, val),
        }
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
