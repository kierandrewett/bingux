#[cfg(test)]
mod tests {
    use crate::error::RecipeError;
    use crate::parser;
    use crate::parse_recipe;

    /// A full source recipe (with build + package functions).
    const FULL_SOURCE_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="firefox"
pkgver="128.0.1"
pkgarch="x86_64-linux"
pkgdesc="Mozilla Firefox web browser"
license="MPL-2.0"

depends=("glibc-2.39" "gtk3-3.24" "dbus-1.14" "pulseaudio-17.0")
makedepends=("rust-1.79" "cbindgen-0.26" "nodejs-20" "nasm-2.16")

exports=(
    "bin/firefox"
    "lib/libxul.so"
    "share/applications/firefox.desktop"
)

source=("https://archive.mozilla.org/pub/firefox/releases/${pkgver}/linux-x86_64/en-GB/firefox-${pkgver}.tar.bz2")
sha256sums=("abc123def456")

dlopen_hints=("libpulse.so=/system/packages/pulseaudio-*/lib/")

build() {
    cd "$SRCDIR/firefox-${pkgver}"
    ./mach configure --prefix=/
    ./mach build
}

package() {
    cd "$SRCDIR/firefox-${pkgver}"
    DESTDIR="$PKGDIR" ./mach install
}
"#;

    /// A binary recipe (no build function).
    const BINARY_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="hello"
pkgver="1.0"
pkgarch="x86_64-linux"
pkgdesc="A simple hello world"

depends=("glibc-2.39")

source=("https://example.com/hello-1.0.tar.gz")
sha256sums=("deadbeef")

package() {
    cp hello "$PKGDIR/bin/"
}
"#;

    // ─── Valid recipe tests ──────────────────────────────────────

    #[test]
    fn parse_full_source_recipe() {
        let recipe = parse_recipe(FULL_SOURCE_RECIPE).unwrap();

        assert_eq!(recipe.pkgscope, "bingux");
        assert_eq!(recipe.pkgname, "firefox");
        assert_eq!(recipe.pkgver, "128.0.1");
        assert_eq!(recipe.pkgarch, "x86_64-linux");
        assert_eq!(recipe.pkgdesc.as_deref(), Some("Mozilla Firefox web browser"));
        assert_eq!(recipe.license.as_deref(), Some("MPL-2.0"));

        assert_eq!(
            recipe.depends,
            vec!["glibc-2.39", "gtk3-3.24", "dbus-1.14", "pulseaudio-17.0"]
        );
        assert_eq!(
            recipe.makedepends,
            vec!["rust-1.79", "cbindgen-0.26", "nodejs-20", "nasm-2.16"]
        );
        assert_eq!(
            recipe.exports,
            vec![
                "bin/firefox",
                "lib/libxul.so",
                "share/applications/firefox.desktop"
            ]
        );

        assert_eq!(recipe.dlopen_hints, vec!["libpulse.so=/system/packages/pulseaudio-*/lib/"]);

        assert!(recipe.build.is_some());
        assert!(recipe.package.is_some());
    }

    #[test]
    fn parse_binary_recipe() {
        let recipe = parse_recipe(BINARY_RECIPE).unwrap();

        assert_eq!(recipe.pkgname, "hello");
        assert_eq!(recipe.pkgver, "1.0");
        assert!(recipe.build.is_none());
        assert!(recipe.package.is_some());
    }

    // ─── Variable expansion ──────────────────────────────────────

    #[test]
    fn variable_expansion_in_sources() {
        let recipe = parse_recipe(FULL_SOURCE_RECIPE).unwrap();
        assert_eq!(
            recipe.source,
            vec![
                "https://archive.mozilla.org/pub/firefox/releases/128.0.1/linux-x86_64/en-GB/firefox-128.0.1.tar.bz2"
            ]
        );
    }

    #[test]
    fn variable_expansion_braces() {
        let vars = std::collections::HashMap::from([
            ("pkgver".to_string(), "2.0".to_string()),
            ("pkgname".to_string(), "test".to_string()),
        ]);
        let result = crate::parser::expand_variables(
            "https://example.com/${pkgname}-${pkgver}.tar.gz",
            &vars,
        )
        .unwrap();
        assert_eq!(result, "https://example.com/test-2.0.tar.gz");
    }

    #[test]
    fn variable_expansion_bare_dollar() {
        let vars = std::collections::HashMap::from([
            ("FOO".to_string(), "bar".to_string()),
        ]);
        let result = crate::parser::expand_variables("$FOO/path", &vars).unwrap();
        assert_eq!(result, "bar/path");
    }

    #[test]
    fn unknown_variables_preserved() {
        let vars = std::collections::HashMap::new();
        let result = crate::parser::expand_variables("$SRCDIR/${PKGDIR}", &vars).unwrap();
        assert_eq!(result, "$SRCDIR/${PKGDIR}");
    }

    // ─── Comments ────────────────────────────────────────────────

    #[test]
    fn comments_are_ignored() {
        let input = r#"
# This is a comment
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"

# Another comment
package() {
    echo "hello"
}
"#;
        let recipe = parse_recipe(input).unwrap();
        assert_eq!(recipe.pkgname, "test");
    }

    // ─── Multiline arrays ────────────────────────────────────────

    #[test]
    fn multiline_array() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"

depends=(
    "foo-1.0"
    "bar-2.0"
    "baz-3.0"
)

package() {
    true
}
"#;
        let recipe = parse_recipe(input).unwrap();
        assert_eq!(recipe.depends, vec!["foo-1.0", "bar-2.0", "baz-3.0"]);
    }

    #[test]
    fn single_line_array() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"
depends=("a" "b" "c")

package() {
    true
}
"#;
        let recipe = parse_recipe(input).unwrap();
        assert_eq!(recipe.depends, vec!["a", "b", "c"]);
    }

    #[test]
    fn empty_array() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"
depends=()

package() {
    true
}
"#;
        let recipe = parse_recipe(input).unwrap();
        assert!(recipe.depends.is_empty());
    }

    // ─── Function body parsing ───────────────────────────────────

    #[test]
    fn function_body_captured() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"

build() {
    cd "$SRCDIR"
    make
}

package() {
    make install DESTDIR="$PKGDIR"
}
"#;
        let recipe = parse_recipe(input).unwrap();
        let build = recipe.build.unwrap();
        assert!(build.contains("cd \"$SRCDIR\""));
        assert!(build.contains("make"));

        let package = recipe.package.unwrap();
        assert!(package.contains("make install"));
    }

    #[test]
    fn nested_braces_in_function() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"

package() {
    if [ -d "$PKGDIR" ]; then
        for f in *.so; do
            install -Dm755 "$f" "$PKGDIR/lib/$f"
        done
    fi
}
"#;
        let recipe = parse_recipe(input).unwrap();
        let body = recipe.package.unwrap();
        // The body should contain the full nested structure, not truncate at the
        // first `}`.
        assert!(body.contains("for f in"));
        assert!(body.contains("done"));
        assert!(body.contains("fi"));
    }

    // ─── Validation errors ───────────────────────────────────────

    #[test]
    fn missing_pkgname() {
        let input = r#"
pkgver="1.0"
pkgarch="x86_64-linux"
package() { true; }
"#;
        let err = parse_recipe(input).unwrap_err();
        assert!(matches!(err, RecipeError::MissingField(ref f) if f == "pkgname"));
    }

    #[test]
    fn missing_pkgver() {
        let input = r#"
pkgname="test"
pkgarch="x86_64-linux"
package() { true; }
"#;
        let err = parse_recipe(input).unwrap_err();
        assert!(matches!(err, RecipeError::MissingField(ref f) if f == "pkgver"));
    }

    #[test]
    fn missing_pkgarch() {
        let input = r#"
pkgname="test"
pkgver="1.0"
package() { true; }
"#;
        let err = parse_recipe(input).unwrap_err();
        assert!(matches!(err, RecipeError::MissingField(ref f) if f == "pkgarch"));
    }

    #[test]
    fn missing_package_function() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"
"#;
        let err = parse_recipe(input).unwrap_err();
        assert!(matches!(err, RecipeError::MissingField(ref f) if f == "package()"));
    }

    #[test]
    fn unknown_arch_rejected() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="sparc-solaris"
package() { true; }
"#;
        let err = parse_recipe(input).unwrap_err();
        assert!(matches!(err, RecipeError::ValidationError(_)));
    }

    #[test]
    fn source_sha256_count_mismatch() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"
source=("https://a.com/a.tar.gz" "https://b.com/b.tar.gz")
sha256sums=("abc123")

package() { true; }
"#;
        let err = parse_recipe(input).unwrap_err();
        assert!(matches!(err, RecipeError::ValidationError(_)));
    }

    // ─── Syntax errors ───────────────────────────────────────────

    #[test]
    fn unterminated_array() {
        let input = r#"
pkgname="test"
depends=("a" "b"
"#;
        let err = parser::parse(input).unwrap_err();
        assert!(matches!(err, RecipeError::SyntaxError { .. }));
    }

    #[test]
    fn unterminated_function() {
        let input = r#"
pkgname="test"
build() {
    make
"#;
        let err = parser::parse(input).unwrap_err();
        assert!(matches!(err, RecipeError::SyntaxError { .. }));
    }

    // ─── Edge cases ──────────────────────────────────────────────

    #[test]
    fn unquoted_scalar_value() {
        let input = r#"
pkgname=test
pkgver=1.0
pkgarch=x86_64-linux

package() {
    true
}
"#;
        let recipe = parse_recipe(input).unwrap();
        assert_eq!(recipe.pkgname, "test");
        assert_eq!(recipe.pkgver, "1.0");
    }

    #[test]
    fn build_function_optional_for_binary() {
        let recipe = parse_recipe(BINARY_RECIPE).unwrap();
        assert!(recipe.build.is_none());
    }

    #[test]
    fn function_with_compact_syntax() {
        // `package(){` with no space before `{`
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"

package(){
    echo done
}
"#;
        let recipe = parse_recipe(input).unwrap();
        assert!(recipe.package.is_some());
    }

    #[test]
    fn scope_defaults_to_empty_when_omitted() {
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"
package() { true; }
"#;
        let recipe = parse_recipe(input).unwrap();
        assert_eq!(recipe.pkgscope, "");
    }

    #[test]
    fn inline_function_body() {
        // Single-line function: `package() { cp a b; }`
        let input = r#"
pkgname="test"
pkgver="1.0"
pkgarch="x86_64-linux"
package() { cp a b; }
"#;
        let recipe = parse_recipe(input).unwrap();
        assert!(recipe.package.is_some());
    }

    #[test]
    fn variable_expansion_in_array() {
        let input = r#"
pkgver="5.0"
pkgname="test"
pkgarch="x86_64-linux"

source=("https://example.com/test-${pkgver}.tar.gz")
sha256sums=("aaa")

package() { true; }
"#;
        let recipe = parse_recipe(input).unwrap();
        assert_eq!(
            recipe.source,
            vec!["https://example.com/test-5.0.tar.gz"]
        );
    }
}
