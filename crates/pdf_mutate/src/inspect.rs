//! Hidden-content discovery (read-only): optional content groups (layers)
//! declared in the catalog's /OCProperties. The other hidden-content
//! categories are covered elsewhere — comments via `read::list_annotations`,
//! invisible text via the pdfium engine (text render mode 3).

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};
use pdf_types::LayerInfo;

fn resolve<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
    match obj {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(obj),
        other => other,
    }
}

/// List every optional-content group (layer) in the document. `hidden` is
/// taken from the default configuration's /OFF list — those layers carry
/// content that exists in the file but is not displayed.
pub fn list_layers(doc: &Document) -> Vec<LayerInfo> {
    let Some(catalog) = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|id| doc.get_dictionary(id).ok())
    else {
        return Vec::new();
    };
    let Some(ocp) = catalog
        .get(b"OCProperties")
        .ok()
        .map(|o| resolve(doc, o))
        .and_then(|o| o.as_dict().ok())
    else {
        return Vec::new();
    };

    let mut off: HashSet<ObjectId> = HashSet::new();
    if let Some(cfg) = ocp
        .get(b"D")
        .ok()
        .map(|o| resolve(doc, o))
        .and_then(|o| o.as_dict().ok())
    {
        if let Some(arr) = cfg
            .get(b"OFF")
            .ok()
            .map(|o| resolve(doc, o))
            .and_then(|o| o.as_array().ok())
        {
            for o in arr {
                if let Ok(id) = o.as_reference() {
                    off.insert(id);
                }
            }
        }
    }

    let mut out = Vec::new();
    if let Some(arr) = ocp
        .get(b"OCGs")
        .ok()
        .map(|o| resolve(doc, o))
        .and_then(|o| o.as_array().ok())
    {
        for o in arr {
            let id = o.as_reference().ok();
            let Some(dict) = resolve(doc, o).as_dict().ok() else {
                continue;
            };
            let name = dict
                .get(b"Name")
                .ok()
                .map(|n| resolve(doc, n))
                .and_then(|n| n.as_str().ok())
                .map(crate::read::decode_pdf_string)
                .unwrap_or_else(|| "?".into());
            out.push(LayerInfo {
                name,
                hidden: id.is_some_and(|i| off.contains(&i)),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Dictionary};

    #[test]
    fn lists_layers_with_hidden_state() {
        let mut d = Document::with_version("1.5");
        let visible_id = d.add_object(Object::Dictionary(dictionary! {
            "Type" => "OCG",
            "Name" => Object::string_literal("Wasserzeichen"),
        }));
        let hidden_id = d.add_object(Object::Dictionary(dictionary! {
            "Type" => "OCG",
            "Name" => Object::string_literal("Interna"),
        }));
        let mut cat = Dictionary::new();
        cat.set("Type", Object::Name(b"Catalog".to_vec()));
        cat.set(
            "OCProperties",
            Object::Dictionary(dictionary! {
                "OCGs" => Object::Array(vec![
                    Object::Reference(visible_id),
                    Object::Reference(hidden_id),
                ]),
                "D" => Object::Dictionary(dictionary! {
                    "OFF" => Object::Array(vec![Object::Reference(hidden_id)]),
                }),
            }),
        );
        let cat_id = d.add_object(Object::Dictionary(cat));
        d.trailer.set("Root", Object::Reference(cat_id));

        let layers = list_layers(&d);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].name, "Wasserzeichen");
        assert!(!layers[0].hidden);
        assert_eq!(layers[1].name, "Interna");
        assert!(layers[1].hidden);
    }

    #[test]
    fn no_ocproperties_means_no_layers() {
        let mut d = Document::with_version("1.5");
        let cat_id = d.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
        }));
        d.trailer.set("Root", Object::Reference(cat_id));
        assert!(list_layers(&d).is_empty());
    }
}
