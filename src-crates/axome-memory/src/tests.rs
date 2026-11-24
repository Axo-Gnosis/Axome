use crate::{
    edit, format,
    index::{EMBED_DIM, EMBED_MODEL},
    links,
    search::{self, SearchQuery},
    sync::sync_bank,
    temporal, Index,
};
use std::{fs, path::Path};
fn setup() -> (tempfile::TempDir, Index) {
    let dir = tempfile::tempdir().unwrap();
    let index = Index::open(&dir.path().join("index.db")).unwrap();
    (dir, index)
}
fn put(dir: &Path, name: &str, text: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(name), text).unwrap();
}
#[test]
fn replace_line_edits_only_the_first_exact_match() {
    let text = "other old\r\nold\r\nold";
    assert_eq!(
        edit::replace_line(text, "old", None).as_deref(),
        Some("other old\r\nold")
    );
    assert_eq!(
        edit::replace_line(text, "old", Some("new")).as_deref(),
        Some("other old\r\nnew\r\nold")
    );
    assert_eq!(edit::replace_line(text, "other", None), None);
}

#[test]
fn format_fixture() {
    let text="---\ntitle: Example\ntype: project\ntags: [rust]\ncustom: {keep: true}\n---\n## Observations\n- [fact] @valid[2026-01-01,2026-02-01) Uses Rust #a#b (source)\n* [decision] @2026-11 Good choice\n+ [fact] @due:2026-12-01 Deadline\n- [fact] @unknown:tomorrow keep qualifier\n- [ ] task\n- [x] done\n- [X] done\n- [-] cancelled\n- [01:02:03] timestamp\n> - [fact] quoted\n```\n- [fact] code [[Hidden]]\n```\n~~~\n[[Also Hidden]]\n~~~\n- depends_on [[Other]] (reason)\n- 'related to' [[Third]]\nProse [[Fourth]] `[[Code]]` \\[[Escaped]]\n- [fact] Links [[Fifth]]\n";
    let note = format::parse(text);
    assert_eq!(note.frontmatter.title.as_deref(), Some("Example"));
    assert_eq!(note.observations.len(), 5);
    assert_eq!(note.relations.len(), 4);
    assert_eq!(note.observations[0].tags, ["a", "b"]);
    assert_eq!(note.observations[0].context.as_deref(), Some("source"));
    assert_eq!(note.observations[0].line, 8);
    assert_eq!(
        note.observations[3].content,
        "@unknown:tomorrow keep qualifier"
    );
    assert!(note.observations[3].time.is_none());
    assert_eq!(note.relations[1].relation_type, "related to");
    let rendered = format::render_frontmatter(&note.frontmatter);
    assert_eq!(
        format::parse(&rendered).frontmatter.extra,
        note.frontmatter.extra
    );
    let invalid = "---\n[broken\n---\nbody";
    assert_eq!(format::parse(invalid).body, invalid);
    assert_eq!(edit::append_observation(invalid, "- [fact] new"), invalid);
    assert_eq!(
        crate::permalink::permalink("docs/My Feature.md"),
        "docs/my-feature"
    );
    assert!(temporal::contains(
        note.observations[0].time.as_ref().unwrap(),
        "2026-01-15"
    ));
    assert!(!temporal::contains(
        note.observations[0].time.as_ref().unwrap(),
        "2026-02-01"
    ));
}
#[test]
fn sync_lifecycle() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    put(&bank, "A.md", "- [fact] original\n");
    assert_eq!(sync_bank(&mut index, &bank).unwrap().added, 1);
    let bank_id = index.bank_id(&bank).unwrap().unwrap();
    let id = links::resolve(&index, bank_id, "A").unwrap().unwrap();
    assert!(!sync_bank(&mut index, &bank).unwrap().has_changes());
    put(&bank, "A.md", "- [fact] changed content\n");
    assert_eq!(sync_bank(&mut index, &bank).unwrap().changed, 1);
    let chunks = index.chunks_needing_embedding(EMBED_MODEL, 10).unwrap();
    for (id, text) in chunks {
        index
            .set_embedding(id, EMBED_MODEL, &text, &vec![1.0; EMBED_DIM])
            .unwrap();
    }
    fs::rename(bank.join("A.md"), bank.join("Moved.md")).unwrap();
    assert_eq!(sync_bank(&mut index, &bank).unwrap().moved, 1);
    assert_eq!(links::resolve(&index, bank_id, "Moved").unwrap(), Some(id));
    assert!(index
        .chunks_needing_embedding(EMBED_MODEL, 10)
        .unwrap()
        .is_empty());
    fs::remove_file(bank.join("Moved.md")).unwrap();
    assert_eq!(sync_bank(&mut index, &bank).unwrap().deleted, 1);
    assert_eq!(index.note_count(bank_id).unwrap(), 0);
}
#[test]
fn forward_link() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    put(&bank, "A.md", "- depends_on [[B]]\n");
    sync_bank(&mut index, &bank).unwrap();
    let b = index.bank_id(&bank).unwrap().unwrap();
    let a = links::resolve(&index, b, "A").unwrap().unwrap();
    assert!(links::neighbors(&index, a, &|_, _| Ok(None))
        .unwrap()
        .is_empty());
    put(&bank, "B.md", "Target\n");
    sync_bank(&mut index, &bank).unwrap();
    let target = links::resolve(&index, b, "B").unwrap().unwrap();
    let out = links::neighbors(&index, a, &|_, _| Ok(None)).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(
        (out[0].entity_id, out[0].relation_type.as_str()),
        (target, "depends_on")
    );
    assert_eq!(out[0].direction, links::Direction::Outgoing);
    let back = links::neighbors(&index, target, &|_, _| Ok(None)).unwrap();
    assert_eq!(
        (back[0].entity_id, back[0].direction),
        (a, links::Direction::Incoming)
    );
}
#[test]
fn permalinks_stay_unique() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    put(&bank, "My Note.md", "first\n");
    sync_bank(&mut index, &bank).unwrap();
    put(&bank, "my_note.md", "second\n");
    put(&bank, "C.md", "---\npermalink: my-note\n---\nthird\n");
    sync_bank(&mut index, &bank).unwrap();
    let b = index.bank_id(&bank).unwrap().unwrap();
    let path_of = |index: &Index, slug| {
        let id = links::resolve(index, b, slug).unwrap().unwrap();
        index.entity(id).unwrap().unwrap().file_path
    };
    assert_eq!(path_of(&index, "my-note"), "My Note.md");
    assert_eq!(path_of(&index, "my-note-1"), "C.md");
    assert_eq!(path_of(&index, "my-note-2"), "my_note.md");
    put(&bank, "my_note.md", "second, edited\n");
    sync_bank(&mut index, &bank).unwrap();
    assert_eq!(path_of(&index, "my-note-2"), "my_note.md");
    // A deleted holder releases its permalink within the same scan.
    fs::remove_file(bank.join("My Note.md")).unwrap();
    put(&bank, "D.md", "---\npermalink: my-note\n---\nfourth\n");
    sync_bank(&mut index, &bank).unwrap();
    assert_eq!(path_of(&index, "my-note"), "D.md");
}
#[test]
fn fusion_ordering() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    put(&bank, "Both.md", "- [fact] needle\n");
    put(&bank, "Keyword.md", "- [fact] needle\n");
    put(&bank, "Vector.md", "- [fact] unrelated\n");
    sync_bank(&mut index, &bank).unwrap();
    let b = index.bank_id(&bank).unwrap().unwrap();
    let both = links::resolve(&index, b, "Both").unwrap().unwrap();
    let vector = links::resolve(&index, b, "Vector").unwrap().unwrap();
    let mut embedding = vec![0.0; EMBED_DIM];
    embedding[0] = 1.0;
    let chunks: Vec<(i64, String)> = index
        .conn
        .prepare("SELECT id,text FROM chunk WHERE entity_id IN (?,?)")
        .unwrap()
        .query_map([both, vector], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for (id, text) in chunks {
        index
            .set_embedding(id, EMBED_MODEL, &text, &embedding)
            .unwrap();
    }
    let hits = search::search(
        &index,
        &SearchQuery {
            bank_ids: &[b],
            text: "needle",
            query_embedding: Some(&embedding),
            row_types: &["observation"],
            limit: 10,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].title, "Both");
    assert!((hits[0].score - 1.3).abs() < 0.001);
    assert!(hits[1..].iter().all(|h| (h.score - 1.0).abs() < 0.001));
}
#[test]
fn relaxed_keyword_query_ignores_stopwords() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    put(&bank, "B.md", "- [fact] B is the storage layer\n");
    put(&bank, "Bun.md", "- [fact] Bun is our JavaScript runtime\n");
    sync_bank(&mut index, &bank).unwrap();
    let b = index.bank_id(&bank).unwrap().unwrap();
    let titles = |text| {
        search::search(
            &index,
            &SearchQuery {
                bank_ids: &[b],
                text,
                row_types: &["observation"],
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap()
        .into_iter()
        .map(|h| h.title)
        .collect::<Vec<_>>()
    };
    assert_eq!(titles("what JavaScript toolchain is preferred"), ["Bun"]);
    assert_eq!(titles("Java"), ["Bun"]);
    assert!(titles("JavaScript toolchain").is_empty());
    assert!(titles("storage layer 16").is_empty());
}
#[test]
fn close_and_valid_at() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    let original = "## Observations\n- [fact] Old fact\n";
    let closed = edit::close_observation(original, 2, "2026-11-22");
    assert!(closed.contains("- [fact] @(,2026-11-22) Old fact"));
    assert_eq!(edit::close_observation(&closed, 2, "2026-11-23"), closed);
    put(&bank, "A.md", &closed);
    sync_bank(&mut index, &bank).unwrap();
    let b = index.bank_id(&bank).unwrap().unwrap();
    for (at, count) in [("2026-11-21", 1), ("2026-11-22", 0)] {
        let hits = search::search(
            &index,
            &SearchQuery {
                bank_ids: &[b],
                text: "Old",
                row_types: &["observation"],
                valid_at: Some(at),
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hits.len(), count);
        if count == 1 {
            assert_eq!(hits[0].line, Some(2));
        }
    }
}

#[test]
fn replacement_can_be_closed_again() {
    let closed = edit::close_observation("- [fact] @[2026-01-01,) Uses X\n", 1, "2026-03-01");
    assert_eq!(closed, "- [fact] @[2026-01-01,2026-03-01) Uses X\n");
    let note = format::parse(&closed);
    let time = note.observations[0].time.as_ref().unwrap();
    assert!(temporal::contains(time, "2026-02-01"));
    assert!(!temporal::contains(time, "2026-03-01"));
    let bounded = "- [fact] @[2026-01-01,2026-02-01) Uses X\n";
    assert_eq!(edit::close_observation(bounded, 1, "2026-03-01"), bounded);
}

#[test]
fn links_follow_their_text() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    put(&bank, "A.md", "- depends_on [[B]]\n");
    put(&bank, "B.md", "first\n");
    sync_bank(&mut index, &bank).unwrap();
    let bank_id = index.bank_id(&bank).unwrap().unwrap();
    let a = links::resolve(&index, bank_id, "A").unwrap().unwrap();
    let old = links::neighbors(&index, a, &|_, _| Ok(None)).unwrap()[0].entity_id;
    fs::rename(bank.join("B.md"), bank.join("Old.md")).unwrap();
    sync_bank(&mut index, &bank).unwrap();
    put(&bank, "B.md", "second\n");
    sync_bank(&mut index, &bank).unwrap();
    let current = links::neighbors(&index, a, &|_, _| Ok(None)).unwrap()[0].entity_id;
    assert_eq!(
        current,
        links::resolve(&index, bank_id, "B").unwrap().unwrap()
    );
    assert_ne!(current, old);
}

#[test]
fn embedding_skips_recycled_chunks() {
    let (tmp, mut index) = setup();
    let bank = tmp.path().join("bank");
    put(&bank, "A.md", "- [fact] original\n");
    sync_bank(&mut index, &bank).unwrap();
    let (id, text) = index
        .chunks_needing_embedding(EMBED_MODEL, 10)
        .unwrap()
        .remove(0);
    let vector = vec![1.0; EMBED_DIM];
    index
        .set_embedding(id, EMBED_MODEL, "different text", &vector)
        .unwrap();
    assert!(index
        .chunks_needing_embedding(EMBED_MODEL, 10)
        .unwrap()
        .iter()
        .any(|(chunk, _)| *chunk == id));
    index
        .set_embedding(id, EMBED_MODEL, &text, &vector)
        .unwrap();
    assert!(!index
        .chunks_needing_embedding(EMBED_MODEL, 10)
        .unwrap()
        .iter()
        .any(|(chunk, _)| *chunk == id));
}
