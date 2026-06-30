use syn::Field;

/// Extracts the version-range expression from a field's `#[pvn(...)]` attribute, if present.
pub fn version_range(field: &Field) -> Option<syn::Expr> {
    field.attrs.iter().find_map(|attr| {
        if attr.path().is_ident("pvn") {
            Some(attr.parse_args::<syn::Expr>().unwrap())
        } else {
            None
        }
    })
}
