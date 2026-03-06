use crate::diagnostics::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone)]
pub struct Script {
    pub id: NodeId,
    pub span: Span,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Block(BlockStmt),
    Labeled(LabeledStmt),
    If(IfStmt),
    While(WhileStmt),
    DoWhile(DoWhileStmt),
    For(ForStmt),
    ForIn(ForInStmt),
    ForOf(ForOfStmt),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Expression(ExpressionStmt),
    VarDecl(VarDeclStmt),
    LetDecl(LetDeclStmt),
    ConstDecl(ConstDeclStmt),
    FunctionDecl(FunctionDeclStmt),
    ClassDecl(ClassDeclStmt),
    Throw(ThrowStmt),
    Try(TryStmt),
    Switch(SwitchStmt),
    Empty(EmptyStmt),
}

#[derive(Debug, Clone)]
pub struct EmptyStmt {
    pub id: NodeId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LabeledStmt {
    pub id: NodeId,
    pub span: Span,
    pub label: String,
    pub body: Box<Statement>,
}

#[derive(Debug, Clone)]
pub struct ClassDeclStmt {
    pub id: NodeId,
    pub span: Span,
    pub name: String,
    pub superclass: Option<Box<Expression>>,
    pub body: ClassBody,
}

#[derive(Debug, Clone)]
pub struct ClassBody {
    pub span: Span,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub struct ClassMember {
    pub span: Span,
    pub key: ClassMemberKey,
    pub kind: ClassMemberKind,
    pub is_static: bool,
}

#[derive(Debug, Clone)]
pub enum ClassMemberKey {
    Ident(String),
    Computed(Box<Expression>),
}

#[derive(Debug, Clone)]
pub enum ClassMemberKind {
    Method(FunctionExprData),
    Get(FunctionExprData),
    Set(FunctionExprData),
    Field(Option<Box<Expression>>),
}

#[derive(Debug, Clone)]
pub struct SwitchStmt {
    pub id: NodeId,
    pub span: Span,
    pub discriminant: Box<Expression>,
    pub cases: Vec<SwitchCase>,
}

#[derive(Debug, Clone)]
pub struct SwitchCase {
    pub span: Span,
    pub test: Option<Box<Expression>>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct TryStmt {
    pub id: NodeId,
    pub span: Span,
    pub body: Box<Statement>,
    pub catch_param: Option<String>,
    pub catch_body: Option<Box<Statement>>,
    pub finally_body: Option<Box<Statement>>,
}

#[derive(Debug, Clone)]
pub struct ThrowStmt {
    pub id: NodeId,
    pub span: Span,
    pub argument: Box<Expression>,
}

#[derive(Debug, Clone)]
pub struct BlockStmt {
    pub id: NodeId,
    pub span: Span,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub id: NodeId,
    pub span: Span,
    pub condition: Box<Expression>,
    pub then_branch: Box<Statement>,
    pub else_branch: Option<Box<Statement>>,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub id: NodeId,
    pub span: Span,
    pub condition: Box<Expression>,
    pub body: Box<Statement>,
}

#[derive(Debug, Clone)]
pub struct DoWhileStmt {
    pub id: NodeId,
    pub span: Span,
    pub body: Box<Statement>,
    pub condition: Box<Expression>,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub id: NodeId,
    pub span: Span,
    pub init: Option<Box<Statement>>,
    pub condition: Option<Box<Expression>>,
    pub update: Option<Box<Expression>>,
    pub body: Box<Statement>,
}

#[derive(Debug, Clone)]
pub struct ForInStmt {
    pub id: NodeId,
    pub span: Span,
    pub left: ForInOfLeft,
    pub right: Box<Expression>,
    pub body: Box<Statement>,
}

#[derive(Debug, Clone)]
pub struct ForOfStmt {
    pub id: NodeId,
    pub span: Span,
    pub is_await: bool,
    pub left: ForInOfLeft,
    pub right: Box<Expression>,
    pub body: Box<Statement>,
}

#[derive(Debug, Clone)]
pub enum ForInOfLeft {
    VarDecl(String),
    LetDecl(String),
    ConstDecl(String),
    Identifier(String),
    VarBinding(Binding),
    LetBinding(Binding),
    ConstBinding(Binding),
    Pattern(Binding),
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub id: NodeId,
    pub span: Span,
    pub argument: Option<Box<Expression>>,
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub id: NodeId,
    pub span: Span,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContinueStmt {
    pub id: NodeId,
    pub span: Span,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExpressionStmt {
    pub id: NodeId,
    pub span: Span,
    pub expression: Box<Expression>,
}

#[derive(Debug, Clone)]
pub struct VarDeclStmt {
    pub id: NodeId,
    pub span: Span,
    pub declarations: Vec<VarDeclarator>,
}

#[derive(Debug, Clone)]
pub struct LetDeclStmt {
    pub id: NodeId,
    pub span: Span,
    pub declarations: Vec<VarDeclarator>,
}

#[derive(Debug, Clone)]
pub struct ConstDeclStmt {
    pub id: NodeId,
    pub span: Span,
    pub declarations: Vec<VarDeclarator>,
}

#[derive(Debug, Clone)]
pub struct VarDeclarator {
    pub id: NodeId,
    pub span: Span,
    pub binding: Binding,
    pub init: Option<Box<Expression>>,
}

#[derive(Debug, Clone)]
pub enum Binding {
    Ident(String),
    ObjectPattern(Vec<ObjectPatternProp>),
    ArrayPattern(Vec<ArrayPatternElem>),
}

#[derive(Debug, Clone)]
pub enum ObjectPatternTarget {
    Ident(String),
    Expr(Expression),
    Pattern(Box<Binding>),
}

#[derive(Debug, Clone)]
pub struct ObjectPatternProp {
    pub key: String,
    pub target: ObjectPatternTarget,
    pub shorthand: bool,
    pub default_init: Option<Box<Expression>>,
}

#[derive(Debug, Clone)]
pub struct ArrayPatternElem {
    pub binding: Option<String>,
    pub default_init: Option<Box<Expression>>,
    pub rest: bool,
}

impl Binding {
    pub fn names(&self) -> Vec<&str> {
        match self {
            Binding::Ident(n) => vec![n.as_str()],
            Binding::ObjectPattern(props) => props
                .iter()
                .flat_map(|p| match &p.target {
                    ObjectPatternTarget::Ident(n) => vec![n.as_str()],
                    ObjectPatternTarget::Pattern(b) => b.names(),
                    ObjectPatternTarget::Expr(_) => vec![],
                })
                .collect(),
            Binding::ArrayPattern(elems) => {
                elems.iter().filter_map(|e| e.binding.as_deref()).collect()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FunctionDeclStmt {
    pub id: NodeId,
    pub span: Span,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Box<Statement>,
    pub is_generator: bool,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub enum Param {
    Ident(String),
    Default(String, Box<Expression>),
    Rest(String),
    ObjectPattern(Vec<ObjectPatternProp>),
    ArrayPattern(Vec<ArrayPatternElem>),
}

impl Param {
    pub fn name(&self) -> &str {
        match self {
            Param::Ident(n) | Param::Default(n, _) | Param::Rest(n) => n,
            Param::ObjectPattern(_) | Param::ArrayPattern(_) => "",
        }
    }

    pub fn is_rest(&self) -> bool {
        matches!(self, Param::Rest(_))
    }

    pub fn as_binding(&self, index: usize) -> Option<(String, crate::frontend::ast::Binding)> {
        match self {
            Param::ObjectPattern(props) => {
                let synthetic = format!("__param_{}__", index);
                Some((
                    synthetic,
                    crate::frontend::ast::Binding::ObjectPattern(props.clone()),
                ))
            }
            Param::ArrayPattern(elems) => {
                let synthetic = format!("__param_{}__", index);
                Some((
                    synthetic,
                    crate::frontend::ast::Binding::ArrayPattern(elems.clone()),
                ))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expression {
    Literal(LiteralExpr),
    This(ThisExpr),
    Identifier(IdentifierExpr),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Call(CallExpr),
    Assign(AssignExpr),
    Conditional(ConditionalExpr),
    ObjectLiteral(ObjectLiteralExpr),
    ArrayLiteral(ArrayLiteralExpr),
    Member(MemberExpr),
    FunctionExpr(FunctionExprData),
    ArrowFunction(ArrowFunctionExpr),
    PrefixIncrement(PostfixExpr),
    PrefixDecrement(PostfixExpr),
    PostfixIncrement(PostfixExpr),
    PostfixDecrement(PostfixExpr),
    New(NewExpr),
    ClassExpr(ClassExprData),
    LogicalAssign(LogicalAssignExpr),
    Super(SuperExpr),
    Yield(YieldExpr),
    Await(AwaitExpr),
}

#[derive(Debug, Clone)]
pub struct SuperExpr {
    pub id: NodeId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct YieldExpr {
    pub id: NodeId,
    pub span: Span,
    pub argument: Option<Box<Expression>>,
    pub delegate: bool,
}

#[derive(Debug, Clone)]
pub struct AwaitExpr {
    pub id: NodeId,
    pub span: Span,
    pub argument: Box<Expression>,
}

#[derive(Debug, Clone)]
pub struct ClassExprData {
    pub id: NodeId,
    pub span: Span,
    pub name: Option<String>,
    pub superclass: Option<Box<Expression>>,
    pub body: ClassBody,
}

#[derive(Debug, Clone)]
pub struct NewExpr {
    pub id: NodeId,
    pub span: Span,
    pub callee: Box<Expression>,
    pub args: Vec<CallArg>,
}

#[derive(Debug, Clone)]
pub struct PostfixExpr {
    pub id: NodeId,
    pub span: Span,
    pub argument: Box<Expression>,
}

#[derive(Debug, Clone)]
pub struct FunctionExprData {
    pub id: NodeId,
    pub span: Span,
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub body: Box<Statement>,
    pub is_generator: bool,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct ArrowFunctionExpr {
    pub id: NodeId,
    pub span: Span,
    pub params: Vec<Param>,
    pub body: ArrowBody,
}

#[derive(Debug, Clone)]
pub enum ArrowBody {
    Expression(Box<Expression>),
    Block(Box<Statement>),
}

#[derive(Debug, Clone)]
pub struct ObjectLiteralExpr {
    pub id: NodeId,
    pub span: Span,
    pub properties: Vec<ObjectPropertyOrSpread>,
}

#[derive(Debug, Clone)]
pub enum ObjectPropertyOrSpread {
    Property(ObjectProperty),
    Spread(Expression),
}

#[derive(Debug, Clone)]
pub struct ObjectProperty {
    pub key: ObjectPropertyKey,
    pub value: Expression,
    pub kind: ObjectPropertyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectPropertyKind {
    Data,
    Get,
    Set,
}

#[derive(Debug, Clone)]
pub enum ObjectPropertyKey {
    Static(String),
    Computed(Expression),
}

#[derive(Debug, Clone)]
pub enum ArrayElement {
    Expr(Expression),
    Hole,
    Spread(Expression),
}

#[derive(Debug, Clone)]
pub struct ArrayLiteralExpr {
    pub id: NodeId,
    pub span: Span,
    pub elements: Vec<ArrayElement>,
}

#[derive(Debug, Clone)]
pub struct MemberExpr {
    pub id: NodeId,
    pub span: Span,
    pub object: Box<Expression>,
    pub property: MemberProperty,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub enum MemberProperty {
    Identifier(String),
    Expression(Box<Expression>),
}

#[derive(Debug, Clone)]
pub struct ThisExpr {
    pub id: NodeId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LiteralExpr {
    pub id: NodeId,
    pub span: Span,
    pub value: LiteralValue,
}

#[derive(Debug, Clone)]
pub enum LiteralValue {
    Null,
    True,
    False,
    Number(f64),
    Int(i64),
    BigInt(String),
    String(String),
    RegExp { pattern: String, flags: String },
}

#[derive(Debug, Clone)]
pub struct IdentifierExpr {
    pub id: NodeId,
    pub span: Span,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub id: NodeId,
    pub span: Span,
    pub op: BinaryOp,
    pub left: Box<Expression>,
    pub right: Box<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Comma,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    LogicalAnd,
    LogicalOr,
    NullishCoalescing,
    LeftShift,
    RightShift,
    UnsignedRightShift,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    Instanceof,
    In,
}

#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub id: NodeId,
    pub span: Span,
    pub op: UnaryOp,
    pub argument: Box<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Minus,
    Plus,
    LogicalNot,
    BitwiseNot,
    Typeof,
    Delete,
    Void,
}

#[derive(Debug, Clone)]
pub enum CallArg {
    Expr(Expression),
    Spread(Expression),
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub id: NodeId,
    pub span: Span,
    pub callee: Box<Expression>,
    pub args: Vec<CallArg>,
}

#[derive(Debug, Clone)]
pub struct AssignExpr {
    pub id: NodeId,
    pub span: Span,
    pub left: Box<Expression>,
    pub right: Box<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalAssignOp {
    Or,
    And,
    Nullish,
}

#[derive(Debug, Clone)]
pub struct LogicalAssignExpr {
    pub id: NodeId,
    pub span: Span,
    pub op: LogicalAssignOp,
    pub left: Box<Expression>,
    pub right: Box<Expression>,
}

#[derive(Debug, Clone)]
pub struct ConditionalExpr {
    pub id: NodeId,
    pub span: Span,
    pub condition: Box<Expression>,
    pub then_expr: Box<Expression>,
    pub else_expr: Box<Expression>,
}

impl Script {
    pub fn span(&self) -> Span {
        self.span
    }
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Statement::Block(s) => s.span,
            Statement::Labeled(s) => s.span,
            Statement::If(s) => s.span,
            Statement::While(s) => s.span,
            Statement::DoWhile(s) => s.span,
            Statement::For(s) => s.span,
            Statement::Return(s) => s.span,
            Statement::Break(s) => s.span,
            Statement::Continue(s) => s.span,
            Statement::Expression(s) => s.span,
            Statement::VarDecl(s) => s.span,
            Statement::LetDecl(s) => s.span,
            Statement::ConstDecl(s) => s.span,
            Statement::FunctionDecl(s) => s.span,
            Statement::ClassDecl(s) => s.span,
            Statement::Throw(s) => s.span,
            Statement::Try(s) => s.span,
            Statement::Switch(s) => s.span,
            Statement::ForIn(s) => s.span,
            Statement::ForOf(s) => s.span,
            Statement::Empty(s) => s.span,
        }
    }
}

impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Expression::Literal(e) => e.span,
            Expression::This(e) => e.span,
            Expression::Identifier(e) => e.span,
            Expression::Binary(e) => e.span,
            Expression::Unary(e) => e.span,
            Expression::Call(e) => e.span,
            Expression::Assign(e) => e.span,
            Expression::Conditional(e) => e.span,
            Expression::ObjectLiteral(e) => e.span,
            Expression::ArrayLiteral(e) => e.span,
            Expression::Member(e) => e.span,
            Expression::FunctionExpr(e) => e.span,
            Expression::ArrowFunction(e) => e.span,
            Expression::PrefixIncrement(e)
            | Expression::PrefixDecrement(e)
            | Expression::PostfixIncrement(e)
            | Expression::PostfixDecrement(e) => e.span,
            Expression::New(e) => e.span,
            Expression::ClassExpr(e) => e.span,
            Expression::LogicalAssign(e) => e.span,
            Expression::Super(e) => e.span,
            Expression::Yield(e) => e.span,
            Expression::Await(e) => e.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_equality() {
        assert_eq!(NodeId(0), NodeId(0));
        assert_ne!(NodeId(0), NodeId(1));
    }

    #[test]
    fn literal_value_variants() {
        let _ = LiteralValue::Int(42);
        let _ = LiteralValue::Number(3.14);
        let _ = LiteralValue::String("hi".to_string());
    }
}
