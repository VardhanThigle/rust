//! Meta-syntax validation logic of attributes for post-expansion.

use errors::{PResult, Applicability};
use rustc_feature::{AttributeTemplate, BUILTIN_ATTRIBUTE_MAP};
use syntax::ast::{self, Attribute, AttrKind, Ident, MetaItem, MetaItemKind};
use syntax::attr::mk_name_value_item_str;
use syntax::early_buffered_lints::BufferedEarlyLintId;
use syntax::token;
use syntax::tokenstream::TokenTree;
use syntax::sess::ParseSess;
use syntax_pos::{Symbol, sym};

pub fn check_meta(sess: &ParseSess, attr: &Attribute) {
    let attr_info =
        attr.ident().and_then(|ident| BUILTIN_ATTRIBUTE_MAP.get(&ident.name)).map(|a| **a);

    // Check input tokens for built-in and key-value attributes.
    match attr_info {
        // `rustc_dummy` doesn't have any restrictions specific to built-in attributes.
        Some((name, _, template, _)) if name != sym::rustc_dummy =>
            check_builtin_attribute(sess, attr, name, template),
        _ => if let Some(TokenTree::Token(token)) = attr.get_normal_item().tokens.trees().next() {
            if token == token::Eq {
                // All key-value attributes are restricted to meta-item syntax.
                parse_meta(sess, attr).map_err(|mut err| err.emit()).ok();
            }
        }
    }
}

pub fn parse_meta<'a>(sess: &'a ParseSess, attr: &Attribute) -> PResult<'a, MetaItem> {
    Ok(match attr.kind {
        AttrKind::Normal(ref item) => MetaItem {
            path: item.path.clone(),
            kind: super::parse_in_attr(sess, attr, |p| p.parse_meta_item_kind())?,
            span: attr.span,
        },
        AttrKind::DocComment(comment) => {
            mk_name_value_item_str(Ident::new(sym::doc, attr.span), comment, attr.span)
        }
    })
}

/// Checks that the given meta-item is compatible with this `AttributeTemplate`.
fn is_attr_template_compatible(template: &AttributeTemplate, meta: &ast::MetaItemKind) -> bool {
    match meta {
        MetaItemKind::Word => template.word,
        MetaItemKind::List(..) => template.list.is_some(),
        MetaItemKind::NameValue(lit) if lit.kind.is_str() => template.name_value_str.is_some(),
        MetaItemKind::NameValue(..) => false,
    }
}

pub fn check_builtin_attribute(
    sess: &ParseSess,
    attr: &Attribute,
    name: Symbol,
    template: AttributeTemplate,
) {
    // Some special attributes like `cfg` must be checked
    // before the generic check, so we skip them here.
    let should_skip = |name| name == sym::cfg;
    // Some of previously accepted forms were used in practice,
    // report them as warnings for now.
    let should_warn = |name| name == sym::doc || name == sym::ignore ||
                             name == sym::inline || name == sym::link ||
                             name == sym::test || name == sym::bench;

    match parse_meta(sess, attr) {
        Ok(meta) => if !should_skip(name) && !is_attr_template_compatible(&template, &meta.kind) {
            let error_msg = format!("malformed `{}` attribute input", name);
            let mut msg = "attribute must be of the form ".to_owned();
            let mut suggestions = vec![];
            let mut first = true;
            if template.word {
                first = false;
                let code = format!("#[{}]", name);
                msg.push_str(&format!("`{}`", &code));
                suggestions.push(code);
            }
            if let Some(descr) = template.list {
                if !first {
                    msg.push_str(" or ");
                }
                first = false;
                let code = format!("#[{}({})]", name, descr);
                msg.push_str(&format!("`{}`", &code));
                suggestions.push(code);
            }
            if let Some(descr) = template.name_value_str {
                if !first {
                    msg.push_str(" or ");
                }
                let code = format!("#[{} = \"{}\"]", name, descr);
                msg.push_str(&format!("`{}`", &code));
                suggestions.push(code);
            }
            if should_warn(name) {
                sess.buffer_lint(
                    BufferedEarlyLintId::IllFormedAttributeInput,
                    meta.span,
                    ast::CRATE_NODE_ID,
                    &msg,
                );
            } else {
                sess.span_diagnostic.struct_span_err(meta.span, &error_msg)
                    .span_suggestions(
                        meta.span,
                        if suggestions.len() == 1 {
                            "must be of the form"
                        } else {
                            "the following are the possible correct uses"
                        },
                        suggestions.into_iter(),
                        Applicability::HasPlaceholders,
                    ).emit();
            }
        }
        Err(mut err) => err.emit(),
    }
}
