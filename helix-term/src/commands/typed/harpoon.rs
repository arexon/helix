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

    let index = index(args.first())?;

    let (view, doc) = current!(cx.editor);
    let path = path::get_relative_path(
        doc.path()
            .ok_or_else(|| anyhow!("current document has no path"))?,
    );
    let selection = doc.selection(view.id);

    let mut store = Store::open()?;
    store.set_file(index, File::new(path.clone(), selection));
    store.save()?;

    let path_str = path.to_string_lossy().to_string();
    cx.editor
        .set_status(format!("'{}' added to #{}", path_str, index));

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

    let index = index(args.first())?;

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

pub fn remove(
    cx: &mut compositor::Context,
    args: &[Cow<str>],
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let index = index(args.first())?;

    let mut store = Store::open()?;
    let file = store.remove_file(index);
    if let Some(file) = file {
        cx.editor.set_status(format!(
            "'{}' removed from #{}",
            file.path.to_string_lossy(),
            index
        ));
        store.save()?;
    } else {
        cx.editor
            .set_error(format!("No file assigned to #{}", index));
    }

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
    let path = path::get_relative_path(path);

    let mut store = Store::open()?;
    let project = store.project();
    if let Some(file) = project.files.values_mut().find(|file| file.path == path) {
        let selection = doc.selection(view.id);
        file.update_selection(selection);
    }
    store.save()?;

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
    let mut files = project.files.iter().collect::<Vec<_>>();
    files.sort_by(|(a_index, _), (b_index, _)| a_index.cmp(b_index));
    let contents = files
        .iter()
        .fold(String::new(), |mut output, (index, file)| {
            let _ = writeln!(output, "{}. {}", index, file.path.to_string_lossy());
            output
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
struct Store<'a> {
    projects: HashMap<PathBuf, Project<'a>>,
}

impl<'a> Store<'a> {
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

    fn set_file(&mut self, index: usize, file: File<'a>) {
        let project = self.project();
        project.files.insert(index, file);
    }

    fn remove_file(&mut self, index: usize) -> Option<File> {
        let project = self.project();
        project.files.remove(&index)
    }

    fn file(&mut self, index: usize) -> Option<&File> {
        let project = self.project();
        let file = project.files.get(&index);
        file.filter(|file| file.path.exists())
    }

    fn project(&mut self) -> &mut Project<'a> {
        let cwd = helix_stdx::env::current_working_dir();
        self.projects.entry(cwd.clone()).or_default()
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Project<'a> {
    files: HashMap<usize, File<'a>>,
}

#[derive(Serialize, Deserialize)]
struct File<'a> {
    path: Cow<'a, Path>,
    spans: SmallVec<[Span; 1]>,
}

impl<'a> File<'a> {
    fn new(path: Cow<'a, Path>, selection: &Selection) -> Self {
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

fn index(arg: Option<&Cow<str>>) -> anyhow::Result<usize> {
    arg.ok_or_else(|| anyhow!("index not provided"))?
        .parse::<usize>()
        .map_err(|_| anyhow!("index must be an integer"))
}
