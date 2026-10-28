//! The "Other file types" section of Default applications: every content type
//! that applications declare and no kind covers, with a friendly name and a
//! category, and the toolbar the Installed list has (search, filter, sort).

use std::cmp::Reverse;
use std::collections::HashMap;

use freya::prelude::*;

use bingux_ui::prelude::*;
use bingux_ui::files::{FileIcon, FileKind};
use bingux_ui::shell::{MenuItem, menu_item};

use super::{FileType, LoadMore, ModelState, PAGE, app_name, set_file_type, set_limit};
use crate::apps::AppEntry;

/// The groups the section is split into, in the order they are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Category {
    Images,
    Audio,
    Video,
    Documents,
    Archives,
    Fonts,
    Code,
    Other,
}

const CATEGORIES: [Category; 8] = [
    Category::Images,
    Category::Audio,
    Category::Video,
    Category::Documents,
    Category::Archives,
    Category::Fonts,
    Category::Code,
    Category::Other,
];

impl Category {
    fn label(self) -> &'static str {
        match self {
            Self::Images => "Images",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Documents => "Documents",
            Self::Archives => "Archives",
            Self::Fonts => "Fonts",
            Self::Code => "Code",
            Self::Other => "Other",
        }
    }

    /// The icon the file view draws for this kind of file.
    fn icon(self) -> Icon {
        FileIcon::for_kind(match self {
            Self::Images => FileKind::Image,
            Self::Audio => FileKind::Audio,
            Self::Video => FileKind::Video,
            Self::Documents => FileKind::Document,
            Self::Archives => FileKind::Archive,
            Self::Code => FileKind::Code,
            Self::Fonts => FileKind::Font,
            Self::Other => FileKind::Unknown,
        })
        .glyph
    }
}

/// The category of a content type: the file view's own judgement of what
/// kind of file it is ([`FileKind::of_mime`]), grouped for the list.
fn category_of(mime: &str) -> Category {
    match FileKind::of_mime(mime) {
        FileKind::Image => Category::Images,
        FileKind::Audio => Category::Audio,
        FileKind::Video => Category::Video,
        FileKind::Text
        | FileKind::Pdf
        | FileKind::Spreadsheet
        | FileKind::Presentation
        | FileKind::Document => Category::Documents,
        FileKind::Archive => Category::Archives,
        FileKind::Font => Category::Fonts,
        FileKind::Code => Category::Code,
        FileKind::Folder | FileKind::Executable | FileKind::Unknown => Category::Other,
    }
}

/// The first unlocalised `<comment>` of a type's database file, which is what
/// the shared-mime-info database calls it, and its first extensions.
fn database_entry(mime: &str) -> (Option<String>, String) {
    let Ok(xml) = bingux_ui::runtime::read_to_string(format!("/usr/share/mime/{mime}.xml")) else {
        return (None, String::new());
    };
    let name = xml.find("<comment>").and_then(|start| {
        let start = start + "<comment>".len();
        let end = start + xml[start..].find("</comment>")?;
        let text = xml[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    });
    let mut extensions: Vec<&str> = Vec::new();
    let mut rest = xml.as_str();
    while let Some(at) = rest.find("<glob pattern=\"*.") {
        rest = &rest[at + "<glob pattern=\"*.".len()..];
        let Some(end) = rest.find('"') else { break };
        let extension = &rest[..end];
        if !extension.contains(['*', '?', '[']) && !extensions.contains(&extension) {
            extensions.push(extension);
        }
    }
    let extensions = extensions
        .into_iter()
        .take(3)
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join("  ");
    (name, extensions)
}

/// A readable name made from the type's own name, when the database has none:
/// `application/x-foo-bar` is "Foo bar".
fn derived_name(mime: &str) -> String {
    let subtype = mime.split_once('/').map_or(mime, |(_, subtype)| subtype);
    let subtype = subtype
        .strip_prefix("x-")
        .or_else(|| subtype.strip_prefix("vnd."))
        .unwrap_or(subtype);
    let subtype = subtype.split('+').next().unwrap_or(subtype);
    let words = subtype.replace(['-', '.', '_'], " ");
    let mut letters = words.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => mime.to_string(),
    }
}

/// The name, category and extensions of a content type. Reads the type
/// database, so call it from a loader thread.
pub(super) fn describe(mime: &str) -> (String, Category, String) {
    let (name, extensions) = database_entry(mime);
    (
        name.unwrap_or_else(|| derived_name(mime)),
        category_of(mime),
        extensions,
    )
}

/// Which types the handler filter keeps.
#[derive(Clone, Copy, PartialEq)]
enum HandlerFilter {
    All,
    /// A default application is set.
    Custom,
    /// No default application is set.
    Unset,
}

/// The order of each group's rows.
#[derive(Clone, Copy, PartialEq)]
enum TypeSort {
    Name,
    Handler,
}

/// A type with the strings the list searches and sorts on.
struct Entry<'a> {
    file: &'a FileType,
    handler: Option<String>,
}

#[derive(PartialEq)]
pub(super) struct OtherFileTypes {
    pub model: ModelState,
    pub overlays: Overlays,
    pub apps: Vec<AppEntry>,
    pub types: Vec<FileType>,
}

impl Component for OtherFileTypes {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let query = use_state(String::new);
        let searching = use_state(|| false);
        let category = use_state(|| None::<Category>);
        let handler_filter = use_state(|| HandlerFilter::All);
        let sort = use_state(|| TypeSort::Name);
        let limit = use_state(|| PAGE);

        let needle = query.read().trim().to_lowercase();
        let category_now = *category.read();
        let handler_now = *handler_filter.read();
        let sort_now = *sort.read();

        let mut entries: Vec<Entry> = self
            .types
            .iter()
            .map(|file| Entry {
                file,
                handler: file.current.as_ref().map(|id| app_name(&self.apps, id)),
            })
            .filter(|entry| category_now.is_none_or(|wanted| entry.file.category == wanted))
            .filter(|entry| match handler_now {
                HandlerFilter::All => true,
                HandlerFilter::Custom => entry.handler.is_some(),
                HandlerFilter::Unset => entry.handler.is_none(),
            })
            .filter(|entry| {
                needle.is_empty()
                    || entry.file.title.to_lowercase().contains(&needle)
                    || entry.file.mime.to_lowercase().contains(&needle)
                    || entry
                        .handler
                        .as_ref()
                        .is_some_and(|name| name.to_lowercase().contains(&needle))
            })
            .collect();
        entries.sort_by_key(|entry| {
            let by_handler = match sort_now {
                TypeSort::Name => String::new(),
                // Types with a handler come first, grouped by handler.
                TypeSort::Handler => entry.handler.clone().unwrap_or_default().to_lowercase(),
            };
            (
                entry.file.category,
                Reverse(sort_now == TypeSort::Handler && entry.handler.is_some()),
                by_handler,
                entry.file.title.to_lowercase(),
            )
        });

        // Two types with one name are told apart by their MIME type.
        let mut name_count: HashMap<&str, usize> = HashMap::new();
        for file in &self.types {
            *name_count.entry(file.title.as_str()).or_default() += 1;
        }

        let total = entries.len();
        let visible = total.min(*limit.read());
        let mut groups: Vec<Group> = Vec::new();
        let mut current: Option<(Category, Vec<Element>)> = None;
        for (index, entry) in entries.iter().take(visible).enumerate() {
            let ambiguous = name_count.get(entry.file.title.as_str()).copied().unwrap_or(0) > 1;
            let mut element = Element::from(file_type_row(
                self.model,
                self.overlays,
                &self.apps,
                entry.file,
                ambiguous,
            ));
            if index + 1 == visible && total > visible {
                element = LoadMore {
                    row: element,
                    on_more: (move |()| set_limit(limit, visible + PAGE)).into(),
                }
                .into();
            }
            match &mut current {
                Some((open, rows)) if *open == entry.file.category => rows.push(element),
                _ => {
                    if let Some((done, rows)) = current.take() {
                        groups.push(Group::titled(done.label()).rows(rows));
                    }
                    current = Some((entry.file.category, vec![element]));
                }
            }
        }
        if let Some((done, rows)) = current {
            groups.push(Group::titled(done.label()).rows(rows));
        }

        let search = reveal(
            *searching.read(),
            search_field(query, move |_| set_limit(limit, PAGE)).fill_width(),
        );
        let head = rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S3)])
            .child(group_head(
                "Other file types",
                toolbar(
                    searching,
                    query,
                    category,
                    handler_filter,
                    sort,
                    limit,
                    (category_now, handler_now, sort_now),
                ),
            ))
            .child(search);

        let cards = if groups.is_empty() {
            page_column().child(Group::new(None).row(status(Status::Empty(
                "No file type matches.".to_string(),
            ))))
        } else {
            groups.into_iter().fold(page_column(), |cards, group| cards.child(group))
        };
        rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S3)])
            .child(head)
            .child(cards)
    }
}

/// The header's controls: a menu to filter and sort, then the search toggle.
/// Closing the search clears its text, so no hidden filter stays on.
fn toolbar(
    searching: State<bool>,
    query: State<String>,
    category: State<Option<Category>>,
    handler_filter: State<HandlerFilter>,
    sort: State<TypeSort>,
    limit: State<usize>,
    (category_now, handler_now, sort_now): (Option<Category>, HandlerFilter, TypeSort),
) -> Element {
    let search = icon_button(Icon::Search, move |_| {
        let mut searching = searching;
        let mut query = query;
        let open = *searching.read();
        if open {
            query.set(String::new());
            set_limit(limit, PAGE);
        }
        searching.set(!open);
    })
    .tooltip("Search");
    let pick_category = move |next: Option<Category>| {
        let mut category = category;
        category.set(next);
        set_limit(limit, PAGE);
    };
    let mut categories: Vec<MenuItem> =
        vec![menu_item("All", move |_| pick_category(None)).checked(category_now.is_none())];
    categories.extend(CATEGORIES.into_iter().map(|kind| {
        menu_item(kind.label(), move |_| pick_category(Some(kind)))
            .checked(category_now == Some(kind))
    }));
    let pick_handler = move |next: HandlerFilter| {
        let mut handler_filter = handler_filter;
        handler_filter.set(next);
        set_limit(limit, PAGE);
    };
    let handlers = vec![
        menu_item("All", move |_| pick_handler(HandlerFilter::All))
            .checked(handler_now == HandlerFilter::All),
        menu_item("Has a default", move |_| pick_handler(HandlerFilter::Custom))
            .checked(handler_now == HandlerFilter::Custom),
        menu_item("Not set", move |_| pick_handler(HandlerFilter::Unset))
            .checked(handler_now == HandlerFilter::Unset),
    ];
    let pick_sort = move |next: TypeSort| {
        let mut sort = sort;
        sort.set(next);
        set_limit(limit, PAGE);
    };
    let sorts = vec![
        menu_item("Name", move |_| pick_sort(TypeSort::Name))
            .checked(sort_now == TypeSort::Name),
        menu_item("Application", move |_| pick_sort(TypeSort::Handler))
            .checked(sort_now == TypeSort::Handler),
    ];
    let menu = menu_button(
        "Filter and sort",
        vec![
            submenu("Category", categories),
            submenu("Default application", handlers),
            submenu("Sort by", sorts),
        ],
    )
    .icon(Icon::Filter);
    hstack(S2).child(menu).child(search).into()
}

/// One content type: its name and category icon, and a dropdown of the
/// applications registered for it. The MIME type is the subtitle only when
/// another type has the same name.
fn file_type_row(
    model: ModelState,
    overlays: Overlays,
    apps: &[AppEntry],
    file: &FileType,
    ambiguous: bool,
) -> Row {
    let mut ids: Vec<String> = Vec::new();
    ids.extend(file.candidates.iter().cloned());
    if let Some(id) = &file.current {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    let mut options: Vec<AppChoice> = Vec::new();
    if file.current.is_none() {
        options.push(AppChoice::new(String::new(), "Not set", String::new()));
    }
    for id in &ids {
        let icon = apps
            .iter()
            .find(|app| app.id == *id)
            .map(|app| app.icon.clone())
            .unwrap_or_default();
        options.push(AppChoice::new(id.clone(), app_name(apps, id), icon));
    }
    let mime = file.mime.clone();
    // The extensions are the secondary line; the MIME type stands in when a
    // type has none or shares its name with another. Every row has one line,
    // so every row has the same height.
    let secondary = if ambiguous || file.extensions.is_empty() {
        file.mime.clone()
    } else {
        file.extensions.clone()
    };
    let menu = app_dropdown(
        options,
        file.current.clone().unwrap_or_default(),
        move |app: String| {
            if !app.is_empty() {
                set_file_type(model, overlays, mime.clone(), app);
            }
        },
    )
    .width(Dim::FieldApps)
    .title(file.title.clone());
    row(file.title.clone())
        .leading(file.category.icon())
        .subtitle(secondary)
        .subtitle_one_line()
        .trailing(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_follow_the_type_name() {
        assert_eq!(category_of("image/webp"), Category::Images);
        assert_eq!(category_of("application/pdf"), Category::Documents);
        assert_eq!(category_of("application/epub+zip"), Category::Documents);
        assert_eq!(category_of("application/x-tar"), Category::Archives);
        assert_eq!(category_of("text/x-python"), Category::Code);
        assert_eq!(category_of("application/json"), Category::Code);
        assert_eq!(category_of("application/x-thing"), Category::Other);
    }

    #[test]
    fn a_name_is_made_from_the_type_when_the_database_has_none() {
        assert_eq!(derived_name("application/x-foo-bar"), "Foo bar");
        assert_eq!(derived_name("application/vnd.acme.widget+xml"), "Acme widget");
    }
}
