use crate::frontend::token::{Token, TokenType};

struct Lexer {
    input: String,
    position: usize,
}

impl Lexer {
    // Taking the character at each 'position' and delegating it to the appropriate collector.
    // After all inputs are used, Return None.
    fn new(input: String) -> Self {
        Lexer { input, position: 0 }
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
