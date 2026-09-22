#[derive(Debug)]
pub struct Program {
    pub module: ModuleDecl,
    pub declarations: Vec<Declaration>,
}

//Modules

#[derive(Debug)]
pub struct ModuleDecl {
    pub name: String,
    pub exposures: Vec<Exposure>,
}

#[derive(Debug)]
pub struct Exposure {
    pub name: String,
    pub exposure_type: ExposureType,
}

#[derive(Debug)]
pub enum ExposureType {
    Endpoint,
}

//Possible Declarations we have currently (?)

#[derive(Debug)]
pub enum Declaration {
    Type(TypeDecl),
    Import(ImportDecl),
    Function(FunctionDecl),
    Variable(VariableDecl),
}

//Possible types

#[derive(Debug)]
pub struct TypeDecl {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub field_type: Type,
}

#[derive(Debug)]
pub enum Type {
    Identifier(String, Vec<Type>), //Generic types with arguments
    Function {
        transactional: bool,
        parameters: Vec<Type>,
        return_type: Option<Box<Type>>,
    },
}

//Imports
#[derive(Debug)]
pub struct ImportDecl {
    pub name: String,
    pub alias: Option<String>,
}

//Functions
#[derive(Debug)]
pub struct FunctionDecl {
    pub transactional: bool,
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
}

#[derive(Debug)]
pub struct Parameter {
    pub name: String,
    pub param_type: Type,
}
//Variables!!!

#[derive(Debug)]
pub struct VariableDecl {
    pub name: String,
    pub variable_type: Option<Type>,
    pub value: Expression,
}
//STATEMENTS!!!!!!!!!!

#[derive(Debug)]
pub enum Statement {
    Expression(Expression),

    Assignment {
        name: String,
        value: Expression,
    },

    Return(Option<Expression>),

    If {
        condition: Expression,
        then_block: Block,
        else_branch: Option<ElseBranch>,
    },

    For {
        variable: String,
        iterator: Expression,
        body: Block,
    },

    While {
        condition: Expression,
        body: Block,
    },
}

#[derive(Debug)]
pub enum ElseBranch {
    Block(Block),
    Statement(Box<Statement>),
}

//Expressions/Function calls/operators/primary values

#[derive(Debug)]
pub enum Expression {
    //Operators
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },

    Unary {
        operator: UnaryOperator,
        expression: Box<Expression>,
    },

    //Function calls
    Call {
        function: Box<Expression>,
        type_arguments: Vec<Type>,
        arguments: Vec<Argument>,
        block: Option<Block>,
    },

    //Access
    MemberAccess {
        object: Box<Expression>,
        field: String,
    },

    //Primary values
    Identifier(String),

    Number(f64),

    String(String),

    Boolean(bool),

    TransactionId(u64),

    Array(Vec<Expression>),

    Object(Vec<Expression>),
}

#[derive(Debug)]
pub struct Argument {
    pub name: Option<String>,
    pub value: Expression,
}

//Operators

#[derive(Debug)]
pub enum BinaryOperator {
    Or,
    And,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Debug)]
pub enum UnaryOperator {
    Not,
    Negate,
}
