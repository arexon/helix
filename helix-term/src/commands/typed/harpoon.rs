use serde::{Deserialize, Serialize};

use super::*;

pub fn set(
    cx: &mut compositor::Context,
    args: &[Cow<str>],
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let index = args
        .first()
        .ok_or_else(|| anyhow!("index not provided"))?
        .parse::<usize>()
        .map_err(|_| anyhow!("index must be an integer"))?;
    let (view, doc) = current!(cx.editor);
    let path = doc
        .path()
        .ok_or_else(|| anyhow!("current document has no path"))?;
    let selection = doc.selection(view.id);

    let mut store = Store::open()?;
    store.set_file(index, File::new(path.clone(), selection));
    store.save()?;

    Ok(())
}

pub fn get(
    cx: &mut compositor::Context,
    args: &[Cow<str>],
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let index = args
        .first()
        .ok_or_else(|| anyhow!("index not provided"))?
        .parse::<usize>()
        .map_err(|_| anyhow!("index must be an integer"))?;

    let mut store = Store::open()?;
    let Some(file) = store.file(index) else {
        return Ok(());
    };

    let _ = cx.editor.open(&file.path, Action::Replace)?;
    let (view, doc) = current!(cx.editor);
    doc.set_selection(view.id, file.as_selection());
    align_view(doc, view, Align::Center);

    Ok(())
}

pub fn update(
    cx: &mut compositor::Context,
    _: &[Cow<str>],
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let (view, doc) = current!(cx.editor);
    let Some(path) = doc.path() else {
        return Ok(());
    };

    let mut store = Store::open()?;
    let project = store.project();
    if let Some(file) = project.files.values_mut().find(|file| &file.path == path) {
        let selection = doc.selection(view.id);
        file.update_selection(selection);
    }

    Ok(())
}

pub fn list(
    cx: &mut compositor::Context,
    _: &[Cow<str>],
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let mut store = Store::open()?;
    let project = store.project();
    let contents =
        project
            .files
            .iter()
            .fold("# Harpoon List\n".to_string(), |mut md, (index, file)| {
                md.push_str("- [");
                md.push_str(&index.to_string());
                md.push_str("] ");
                md.push_str(file.path.to_str().unwrap());
                md.push('\n');
                md
            });

    let callback = async move {
        let call: job::Callback = Callback::EditorCompositor(Box::new(
            move |editor: &mut Editor, compositor: &mut Compositor| {
                let contents = ui::Markdown::new(contents, editor.syn_loader.clone());
                let popup = ui::Popup::new("harpoon", contents).auto_close(true);
                compositor.replace_or_push("harpoon", popup);
            },
        ));
        Ok(call)
    };

    cx.jobs.callback(callback);

    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    projects: HashMap<PathBuf, Project>,
}

impl Store {
    fn open() -> anyhow::Result<Self> {
        match std::fs::read_to_string(helix_loader::harpoon_store_file()) {
            Ok(v) => Ok(serde_json::from_str(&v)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
            Err(e) => Err(anyhow!("cannot access harpoon store file: {e}")),
        }
    }

    fn save(self) -> anyhow::Result<()> {
        let store = serde_json::to_string(&self)?;
        std::fs::write(helix_loader::harpoon_store_file(), store)?;
        Ok(())
    }

    fn set_file(&mut self, index: usize, file: File) {
        let project = self.project();
        project.files.insert(index, file);
    }

    fn file(&mut self, index: usize) -> Option<&File> {
        let project = self.project();
        project.files.get(&index)
    }

    fn project(&mut self) -> &mut Project {
        let cwd = helix_stdx::env::current_working_dir();
        self.projects.entry(cwd.clone()).or_default()
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Project {
    files: HashMap<usize, File>,
}

#[derive(Serialize, Deserialize)]
struct File {
    path: PathBuf,
    spans: SmallVec<[Span; 1]>,
}

impl File {
    fn new(path: PathBuf, selection: &Selection) -> Self {
        Self {
            path: path.clone(),
            spans: selection
                .ranges()
                .iter()
                .map(|range| Span {
                    start: range.anchor,
                    end: range.head,
                })
                .collect(),
        }
    }

    fn update_selection(&mut self, selection: &Selection) {
        self.spans = selection
            .ranges()
            .iter()
            .map(|range| Span {
                start: range.anchor,
                end: range.head,
            })
            .collect()
    }

    fn as_selection(&self) -> Selection {
        Selection::new(
            self.spans
                .iter()
                .map(|span| Range::new(span.start, span.end))
                .collect(),
            0,
        )
    }
}

#[derive(Serialize, Deserialize)]
struct Span {
    start: usize,
    end: usize,
}
