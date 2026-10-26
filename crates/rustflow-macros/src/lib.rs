//! Procedural macros for RustFlow's constrained declarative view syntax.

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// Mark a Rust function as a RustFlow component authoring function.
///
/// The function remains an ordinary typed Rust function; its result can be
/// converted with IntoView and used by the browser renderer. The attribute does
/// not adopt React's positional hook model.
#[proc_macro_attribute]
pub fn component(_attributes: TokenStream, item: TokenStream) -> TokenStream {
    let tokens = item.clone().into_iter().collect::<Vec<_>>();
    let Some(function_index) = tokens
        .iter()
        .position(|token| matches!(token, TokenTree::Ident(ident) if ident.to_string() == "fn"))
    else {
        return compile_error(ParseError {
            message: "#[component] can only annotate a Rust function".into(),
            span: Span::call_site(),
        });
    };
    let Some(TokenTree::Ident(function_name)) = tokens.get(function_index + 1) else {
        return compile_error(ParseError {
            message: "#[component] expected a function name".into(),
            span: Span::call_site(),
        });
    };
    let name = function_name.to_string();
    let Some(parameters) = tokens
        .iter()
        .skip(function_index + 2)
        .find_map(|token| match token {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {
                Some(group.stream())
            }
            _ => None,
        })
    else {
        return compile_error(ParseError {
            message: "#[component] expected a parameter list".into(),
            span: function_name.span(),
        });
    };

    let mut parameter_names = Vec::new();
    let mut bindings = String::new();
    for parameter in split_parameters(parameters) {
        if parameter.trim().is_empty() {
            continue;
        }
        let Some((pattern, type_name)) = parameter.split_once(':') else {
            return compile_error(ParseError {
                message:
                    "#[component] parameters must use simple named patterns with explicit types"
                        .into(),
                span: function_name.span(),
            });
        };
        let parameter_name = pattern
            .split_whitespace()
            .last()
            .unwrap_or("")
            .trim_start_matches("r#")
            .to_owned();
        if parameter_name.is_empty()
            || !parameter_name
                .chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric())
        {
            return compile_error(ParseError {
                message: "#[component] parameters must have simple identifier names".into(),
                span: function_name.span(),
            });
        }
        let type_name = type_name
            .trim()
            .replace(" :: ", "::")
            .replace(" ::", "::")
            .replace(":: ", "::");
        let Some(conversion) = property_conversion(&parameter_name, &type_name) else {
            return compile_error(ParseError {
                message: format!(
                    "#[component] does not support prop type {type_name} for {parameter_name}; use String, bool, i32, i64, u32, usize, f64, or EventHandler"
                ),
                span: function_name.span(),
            });
        };
        bindings.push_str(&conversion);
        parameter_names.push(parameter_name);
    }

    let arguments = parameter_names.join(", ");
    let definition_name = format!(
        "__rustflow_component_definition_{}",
        name.to_ascii_lowercase()
    );
    let generated = format!(
        r#"
        #[allow(non_snake_case)]
        {item}
        #[allow(dead_code)]
        pub fn {definition_name}() -> ::core::result::Result<
            ::rustflow::components::ComponentDefinition,
            ::rustflow::components::ComponentError
        > {{
            ::rustflow::components::ComponentDefinition::new({name:?}, move |_context, __props| {{
                {bindings}
                let __view = ::rustflow::view::IntoView::into_view({name}({arguments}))
                    .map_err(::rustflow::components::ComponentError::View)?;
                let __render: ::rustflow::components::ComponentRender =
                    ::std::rc::Rc::new(move |_props, _children| Ok(__view.clone()));
                Ok(__render)
            }})
        }}
        "#,
        item = item,
        name = name,
        definition_name = definition_name,
        bindings = bindings,
        arguments = arguments,
    );
    generated.parse().unwrap_or_else(|_| {
        compile_error(ParseError {
            message: "#[component] could not generate a valid typed component adapter".into(),
            span: function_name.span(),
        })
    })
}

fn split_parameters(parameters: TokenStream) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = TokenStream::new();
    for token in parameters {
        if matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ',') {
            result.push(current.to_string());
            current = TokenStream::new();
        } else {
            current.extend([token]);
        }
    }
    if !current.is_empty() {
        result.push(current.to_string());
    }
    result
}

fn property_conversion(parameter: &str, type_name: &str) -> Option<String> {
    let error = format!("invalid or missing component prop {parameter}");
    let binding = format!(
        "let __value_{parameter} = __props.iter().find(|prop| prop.name() == {parameter:?}).map(|prop| prop.value()).ok_or_else(|| ::rustflow::components::ComponentError::User({error:?}.to_owned()))?;"
    );
    let conversion = match type_name {
        "String" => format!(
            "let {parameter}: String = match __value_{parameter} {{ ::rustflow::view::PropertyValue::Text(value) => value.clone(), _ => return Err(::rustflow::components::ComponentError::User({error:?}.to_owned())), }};"
        ),
        "bool" => format!(
            "let {parameter}: bool = match __value_{parameter} {{ ::rustflow::view::PropertyValue::Boolean(value) => *value, _ => return Err(::rustflow::components::ComponentError::User({error:?}.to_owned())), }};"
        ),
        "i32" | "i64" | "u32" | "usize" => format!(
            "let {parameter}: {type_name} = match __value_{parameter} {{ ::rustflow::view::PropertyValue::Integer(value) => <{type_name} as ::core::convert::TryFrom<i64>>::try_from(*value).map_err(|_| ::rustflow::components::ComponentError::User({error:?}.to_owned()))?, _ => return Err(::rustflow::components::ComponentError::User({error:?}.to_owned())), }};"
        ),
        "f64" => format!(
            "let {parameter}: f64 = match __value_{parameter} {{ ::rustflow::view::PropertyValue::Float(value) => *value, ::rustflow::view::PropertyValue::Integer(value) => *value as f64, _ => return Err(::rustflow::components::ComponentError::User({error:?}.to_owned())), }};"
        ),
        "EventHandler" | "::rustflow::view::EventHandler" => format!(
            "let {parameter}: ::rustflow::view::EventHandler = match __value_{parameter} {{ ::rustflow::view::PropertyValue::Callback(value) => value.clone(), _ => return Err(::rustflow::components::ComponentError::User({error:?}.to_owned())), }};"
        ),
        _ => return None,
    };
    Some(format!("{binding}{conversion}"))
}

#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    match Parser::new(input).parse_roots() {
        Ok(nodes) => {
            if nodes.is_empty() {
                return compile_error(ParseError {
                    message: "view! expects at least one text, expression, or element node".into(),
                    span: Span::call_site(),
                });
            }
            let body = if nodes.len() == 1 {
                nodes[0].emit()
            } else {
                let children = nodes
                    .iter()
                    .map(|node| format!("({})?", node.emit()))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("Ok(::rustflow::view::View::fragment(::std::vec![{children}]))")
            };
            format!(
                "(|| -> ::core::result::Result<::rustflow::view::View, ::rustflow::view::ViewError> {{ {body} }})()"
            )
            .parse()
            .unwrap_or_else(|_| compile_error(ParseError {
                message: "view! could not generate a valid view expression".into(),
                span: Span::call_site(),
            }))
        }
        Err(error) => compile_error(error),
    }
}

#[derive(Debug)]
struct ParseError {
    message: String,
    span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttributeKind {
    Html,
    Property,
    Event,
    Key,
}

struct Attribute {
    name: String,
    kind: AttributeKind,
    value: Option<String>,
}

struct ElementNode {
    name: String,
    component: bool,
    attributes: Vec<Attribute>,
    children: Vec<Node>,
}

enum Node {
    Literal(String),
    Expression(String),
    Dynamic(String),
    Element(ElementNode),
}

impl Node {
    fn emit(&self) -> String {
        match self {
            Self::Literal(expression) => {
                format!("::rustflow::view::IntoView::into_view(({expression}))")
            }
            Self::Expression(expression) => {
                let value = if is_simple_identifier(expression) {
                    format!("({expression}).clone()")
                } else {
                    expression.clone()
                };
                format!("::rustflow::view::IntoView::into_view(({value}))")
            }
            Self::Dynamic(expression) => format!(
                "Ok(::rustflow::view::View::dynamic(move || ::rustflow::view::IntoView::into_view(({expression}))))"
            ),
            Self::Element(element) => element.emit(),
        }
    }
}

fn is_simple_identifier(expression: &str) -> bool {
    let mut chars = expression.trim().chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

impl ElementNode {
    fn emit(&self) -> String {
        let children = self
            .children
            .iter()
            .map(|node| format!("({})?", node.emit()))
            .collect::<Vec<_>>()
            .join(",");

        if self.component {
            let mut properties = Vec::new();
            let mut key = "None".to_owned();
            for attribute in &self.attributes {
                let value = attribute.value.as_deref().unwrap_or("true");
                if attribute.kind == AttributeKind::Key {
                    key = format!("Some(::rustflow::view::Key::from(({value})))");
                    continue;
                }
                if attribute.kind == AttributeKind::Event {
                    properties.push(format!(
                        "::rustflow::view::Property::new({:?}, ::rustflow::view::PropertyValue::Callback(::rustflow::view::EventHandler::new(({value}))))?",
                        format!("on_{}", attribute.name)
                    ));
                    continue;
                }
                properties.push(format!(
                    "::rustflow::view::Property::new({:?}, ({value}))?",
                    attribute.name
                ));
            }
            let properties = properties.join(",");
            format!(
                "::rustflow::view::View::component({:?}, ::std::vec![{properties}], ::std::vec![{children}], {key})",
                self.name
            )
        } else {
            let mut setup = format!(
                "let mut __rustflow_element = ::rustflow::view::Element::new({:?})?;",
                self.name
            );
            for attribute in &self.attributes {
                let value = attribute.value.as_deref().unwrap_or("true");
                match attribute.kind {
                    AttributeKind::Html => setup.push_str(&format!(
                        "__rustflow_element = __rustflow_element.with_attribute({:?}, ::rustflow::view::AttributeValue::from(({value})))?;",
                        attribute.name
                    )),
                    AttributeKind::Property => setup.push_str(&format!(
                        "__rustflow_element = __rustflow_element.with_property({:?}, ({value}))?;",
                        attribute.name
                    )),
                    AttributeKind::Event => setup.push_str(&format!(
                        "__rustflow_element = __rustflow_element.with_event({:?}, ({value}))?;",
                        attribute.name
                    )),
                    AttributeKind::Key => setup.push_str(&format!(
                        "__rustflow_element = __rustflow_element.with_key(::rustflow::view::Key::from(({value})));"
                    )),
                }
            }
            format!(
                "{{ {setup} let __rustflow_children = ::std::vec![{children}]; Ok(__rustflow_element.with_children(__rustflow_children).build()) }}"
            )
        }
    }
}

struct Parser {
    tokens: Vec<TokenTree>,
    cursor: usize,
}

impl Parser {
    fn new(input: TokenStream) -> Self {
        Self {
            tokens: input.into_iter().collect(),
            cursor: 0,
        }
    }

    fn parse_roots(mut self) -> Result<Vec<Node>, ParseError> {
        let mut nodes = Vec::new();
        while self.cursor < self.tokens.len() {
            nodes.push(self.parse_child()?);
        }
        Ok(nodes)
    }

    fn parse_child(&mut self) -> Result<Node, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.error("expected a view node", Span::call_site()));
        };
        match token {
            TokenTree::Punct(punct) if punct.as_char() == '<' => self.parse_element(),
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                self.cursor += 1;
                Ok(Node::Expression(group.stream().to_string()))
            }
            TokenTree::Group(group) => Err(self.error(
                "only brace-delimited Rust expressions are supported as child expressions",
                group.span(),
            )),
            TokenTree::Punct(punct) if punct.as_char() == '@' => {
                self.cursor += 1;
                let keyword = self.take_ident()?;
                if keyword.to_string() != "dynamic" {
                    return Err(self.error(
                        "expected @dynamic followed by a brace expression",
                        keyword.span(),
                    ));
                }
                let Some(TokenTree::Group(group)) = self.peek().cloned() else {
                    return Err(
                        self.error("expected a brace expression after @dynamic", keyword.span())
                    );
                };
                if group.delimiter() != Delimiter::Brace {
                    return Err(
                        self.error("expected a brace expression after @dynamic", group.span())
                    );
                }
                self.cursor += 1;
                Ok(Node::Dynamic(group.stream().to_string()))
            }
            TokenTree::Literal(literal) => {
                self.cursor += 1;
                Ok(Node::Literal(literal.to_string()))
            }
            _ => Err(self.error(
                "text must be a string/number literal or a brace-delimited Rust expression",
                token.span(),
            )),
        }
    }

    fn parse_element(&mut self) -> Result<Node, ParseError> {
        let open = self.take_punct('<')?;
        if self.peek_punct(0, '/') {
            return Err(self.error("unexpected closing tag", open.span()));
        }
        let name = self.take_name()?;
        let component = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase());
        let mut attributes = Vec::new();
        let self_closing;

        loop {
            if self.peek_punct(0, '/') && self.peek_punct(1, '>') {
                self.cursor += 2;
                self_closing = true;
                break;
            }
            if self.peek_punct(0, '>') {
                self.cursor += 1;
                self_closing = false;
                break;
            }
            if self.cursor >= self.tokens.len() {
                return Err(
                    self.error("unterminated element header; expected > or />", open.span())
                );
            }
            attributes.push(self.parse_attribute()?);
        }

        let mut children = Vec::new();
        if !self_closing {
            loop {
                if self.cursor >= self.tokens.len() {
                    return Err(
                        self.error(format!("missing closing tag for <{name}>"), open.span())
                    );
                }
                if self.peek_punct(0, '<') && self.peek_punct(1, '/') {
                    self.cursor += 2;
                    let closing_name = self.take_name()?;
                    let close_span = self.tokens[self.cursor.saturating_sub(1)].span();
                    self.expect_punct('>')?;
                    if closing_name != name {
                        return Err(self.error(
                            format!("closing tag </{closing_name}> does not match <{name}>"),
                            close_span,
                        ));
                    }
                    break;
                }
                children.push(self.parse_child()?);
            }
        }

        Ok(Node::Element(ElementNode {
            name,
            component,
            attributes,
            children,
        }))
    }

    fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        let Some(first) = self.peek().cloned() else {
            return Err(self.error("expected an attribute name", Span::call_site()));
        };
        let span = first.span();
        let mut name = self.take_name()?;
        let mut kind = AttributeKind::Html;

        if self.peek_punct(0, ':') {
            self.cursor += 1;
            let suffix = self.take_name()?;
            match name.as_str() {
                "on" => {
                    kind = AttributeKind::Event;
                    name = suffix;
                }
                "prop" => {
                    kind = AttributeKind::Property;
                    name = suffix;
                }
                _ => {
                    name = format!("{name}:{suffix}");
                }
            }
        }
        if name == "key" && kind == AttributeKind::Html {
            kind = AttributeKind::Key;
        }

        let value = if self.peek_punct(0, '=') {
            self.cursor += 1;
            Some(self.take_value()?)
        } else {
            None
        };
        if matches!(kind, AttributeKind::Event | AttributeKind::Key) && value.is_none() {
            return Err(self.error("this attribute requires a value expression in braces", span));
        }
        Ok(Attribute { name, kind, value } )
    }

    fn take_value(&mut self) -> Result<String, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.error("expected an attribute value", Span::call_site()));
        };
        match token {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                self.cursor += 1;
                Ok(group.stream().to_string())
            }
            TokenTree::Literal(literal) => {
                self.cursor += 1;
                Ok(literal.to_string())
            }
            TokenTree::Ident(ident) if matches!(ident.to_string().as_str(), "true" | "false") => {
                self.cursor += 1;
                Ok(ident.to_string())
            }
            _ => Err(self.error(
                "attribute values must be literals or brace-delimited expressions",
                token.span(),
            )),
        }
    }

    fn take_name(&mut self) -> Result<String, ParseError> {
        let first = self.take_ident()?;
        let mut name = first.to_string();
        while self.peek_punct(0, '-')
            && matches!(self.tokens.get(self.cursor + 1), Some(TokenTree::Ident(_)))
        {
            self.cursor += 1;
            let suffix = self.take_ident()?;
            name.push('-');
            name.push_str(&suffix.to_string());
        }
        Ok(name)
    }

    fn take_ident(&mut self) -> Result<Ident, ParseError> {
        match self.peek().cloned() {
            Some(TokenTree::Ident(ident)) => {
                self.cursor += 1;
                Ok(ident)
            }
            Some(token) => Err(self.error("expected an identifier", token.span())),
            None => Err(self.error("unexpected end of view! input", Span::call_site())),
        }
    }

    fn take_punct(&mut self, ch: char) -> Result<Punct, ParseError> {
        match self.peek().cloned() {
            Some(TokenTree::Punct(punct)) if punct.as_char() == ch => {
                self.cursor += 1;
                Ok(punct)
            }
            Some(token) => Err(self.error(format!("expected {ch}"), token.span())),
            None => Err(self.error(format!("expected {ch}"), Span::call_site())),
        }
    }

    fn expect_punct(&mut self, ch: char) -> Result<(), ParseError> {
        self.take_punct(ch).map(|_| ())
    }

    fn peek(&self) -> Option<&TokenTree> {
        self.tokens.get(self.cursor)
    }

    fn peek_punct(&self, offset: usize, ch: char) -> bool {
        matches!(self.tokens.get(self.cursor + offset), Some(TokenTree::Punct(punct)) if punct.as_char() == ch)
    }

    fn error(&self, message: impl Into<String>, span: Span) -> ParseError {
        ParseError {
            message: message.into(),
            span,
        }
    }
}

fn compile_error(error: ParseError) -> TokenStream {
    let span = error.span;
    let mut ident = Ident::new("compile_error", span);
    ident.set_span(span);
    let mut bang = Punct::new('!', Spacing::Alone);
    bang.set_span(span);
    let mut literal = Literal::string(&error.message);
    literal.set_span(span);
    let mut arguments = TokenStream::new();
    arguments.extend([TokenTree::Literal(literal)]);
    let mut group = Group::new(Delimiter::Parenthesis, arguments);
    group.set_span(span);
    let mut semicolon = Punct::new(';', Spacing::Alone);
    semicolon.set_span(span);
    [
        TokenTree::Ident(ident),
        TokenTree::Punct(bang),
        TokenTree::Group(group),
        TokenTree::Punct(semicolon),
    ]
    .into_iter()
    .collect()
}
