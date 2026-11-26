//! One stable model per list, updated in place and matched by a key (an
//! app's id), so a refresh never rebuilds rows that did not change and a
//! row's state stays with its app, not with its position. Slint creates a
//! row's widgets once per model row: `sync` keeps a row that is still in
//! the list (updating its data), inserts new ones and removes gone ones; a
//! row that moved is taken out and put back, which drops its keyboard focus,
//! so `sync` says when that happened and the view puts the focus back by id
//! (`view::rows::refocus`).
#![forbid(unsafe_code)]

use slint::{Model, VecModel};

/// What a sync did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Synced {
    /// Rows were taken out and put back elsewhere (their widgets are new).
    pub moved: bool,
    /// Rows were added or removed.
    pub resized: bool,
}

/// Makes `model` hold `items` in their order, with as few changes as can
/// be: rows whose key is in both keep their place where they can and are
/// only updated when their data changed.
pub fn sync<T, K>(model: &VecModel<T>, items: Vec<T>, key: impl Fn(&T) -> K) -> Synced
where
    T: Clone + PartialEq + 'static,
    K: PartialEq,
{
    let mut done = Synced::default();
    for (i, item) in items.iter().enumerate() {
        let wanted = key(item);
        let here = (i < model.row_count()).then(|| model.row_data(i)).flatten();
        match here {
            Some(current) if key(&current) == wanted => {
                if current != *item {
                    model.set_row_data(i, item.clone());
                }
            }
            _ => {
                let later = (i + 1..model.row_count())
                    .find(|&j| model.row_data(j).is_some_and(|r| key(&r) == wanted));
                match later {
                    Some(j) => {
                        model.remove(j);
                        model.insert(i, item.clone());
                        done.moved = true;
                    }
                    None => {
                        model.insert(i, item.clone());
                        done.resized = true;
                    }
                }
            }
        }
    }
    while model.row_count() > items.len() {
        model.remove(model.row_count() - 1);
        done.resized = true;
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(model: &VecModel<(String, u32)>) -> Vec<String> {
        model.iter().map(|(k, _)| k).collect()
    }

    fn items(list: &[(&str, u32)]) -> Vec<(String, u32)> {
        list.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn rows_are_updated_in_place_added_removed_and_moved_by_key() {
        let model = VecModel::default();
        let first = sync(&model, items(&[("a", 1), ("b", 1), ("c", 1)]), |r| {
            r.0.clone()
        });
        assert!(first.resized && !first.moved);
        // Same keys, new data: updated where they are.
        let same = sync(&model, items(&[("a", 2), ("b", 1), ("c", 1)]), |r| {
            r.0.clone()
        });
        assert_eq!(same, Synced::default());
        assert_eq!(model.row_data(0), Some(("a".into(), 2)));
        // Reordered: moved, no row lost.
        let moved = sync(&model, items(&[("c", 1), ("a", 2), ("b", 1)]), |r| {
            r.0.clone()
        });
        assert!(moved.moved && !moved.resized);
        assert_eq!(keys(&model), ["c", "a", "b"]);
        // One gone, one new.
        let changed = sync(&model, items(&[("c", 1), ("d", 1)]), |r| r.0.clone());
        assert!(changed.resized);
        assert_eq!(keys(&model), ["c", "d"]);
        let empty = sync(&model, Vec::new(), |r| r.0.clone());
        assert!(empty.resized);
        assert_eq!(model.row_count(), 0);
    }
}
