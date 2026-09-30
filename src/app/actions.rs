//! Stable, localized entry points for desktop commands. Dialogs retain their
//! own draft validation; these routes open them or perform immediate actions.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ActionId {
    NewProject,
    OpenWelcome,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    RenameProject,
    OpenSettings,
    Undo,
    Redo,
    NewBoard,
    NewMaterial,
    EditMaterial,
    EditGrid,
    EditKerf,
    AddCatalog,
    UpdateCatalog,
    BatchDimensions,
    SelectObject,
    ToggleVisibility,
    Group,
    Reparent,
    Ungroup,
    DuplicateAssembly,
    Transform,
    SelectBoard,
    AssignMaterial,
    PositionBoard,
    PlaceFace,
    DuplicateBoard,
    RenameObject,
    EditDimensions,
    SetGrain,
    ToggleBanding,
    SetBanding,
    ApplyBandingPreset,
    NewEdgeBand,
    EditEdgeBand,
    RemoveEdgeBand,
    NewStock,
    EditStock,
    DuplicateStock,
    DeleteStock,
    StockMove,
    EditCutFee,
    EditCurrency,
    NewHardware,
    EditHardware,
    DuplicateHardware,
    DeleteHardware,
    NewHinge,
    EditHinge,
    DeleteHinge,
    NewDoor,
    EditDoor,
    DeleteDoor,
    NewSlides,
    EditSlides,
    DeleteSlides,
    NewFoot,
    RefitSlides,
    RemoveCatalog,
    DeleteObject,
    StartMotion,
    CloseMotion,
    LocateIssue,
    RepairIssue,
    AddIssueStock,
    BeginRepair,
    AcceptRepair,
    CancelRepair,
    StageRepair,
    Unallocate,
    ToggleAllocationLock,
    SelectSheetBoard,
    PlaceUnallocated,
    ReplanSheets,
    AddSuggestedSheets,
    StartOptimization,
    CancelOptimization,
    AcceptOptimization,
    ConfirmKerf,
    OpenHandoff,
    ExportPdf,
    ReplacePdf,
    CancelExport,
    DirtySave,
    DirtyDiscard,
    OverwriteProject,
    Recover,
    DiscardRecovery,
    DeferRecovery,
    ConfirmDialog,
    CancelDialog,
    SetUiLanguage,
    SetMeasurementScope,
    SetMeasurementFrame,
    SetExportMode,
    SetExportLanguage,
    SetExportUnits,
    SetOptimizerObjective,
    SetDoorAngle,
    ViewNavigate,
    ViewMove,
    ViewMeasure,
    ViewBand,
    ViewFrame,
    ViewPreset,
    ViewProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Project,
    Design,
    Stock,
    CutPlan,
    Hardware,
    Handoff,
    Dialog,
}

impl Route {
    fn name(self, language: Language) -> &'static str {
        match (language, self) {
            (Language::En, Self::Project) => "Project",
            (Language::En, Self::Design) => "Design",
            (Language::En, Self::Stock) => "Stock",
            (Language::En, Self::CutPlan) => "Cut plan",
            (Language::En, Self::Hardware) => "Hardware",
            (Language::En, Self::Handoff) => "Handoff",
            (Language::En, Self::Dialog) => "Dialog",
            (Language::PtBr, Self::Project) => "Projeto",
            (Language::PtBr, Self::Design) => "Projeto 3D",
            (Language::PtBr, Self::Stock) => "Estoque",
            (Language::PtBr, Self::CutPlan) => "Plano de corte",
            (Language::PtBr, Self::Hardware) => "Ferragens",
            (Language::PtBr, Self::Handoff) => "Entrega",
            (Language::PtBr, Self::Dialog) => "Janela",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Descriptor {
    pub id: ActionId,
    pub stable_id: &'static str,
    pub key: &'static str,
    pub route: Route,
    pub en_keywords: &'static str,
    pub pt_keywords: &'static str,
}

macro_rules! registry {
    ($( $id:ident => ($key:literal, $route:ident, $en:literal, $pt:literal) ),+ $(,)?) => {
        pub(crate) const ALL: &[Descriptor] = &[
            $(Descriptor { id: ActionId::$id, stable_id: stringify!($id), key: $key, route: Route::$route,
                en_keywords: $en, pt_keywords: $pt }),+
        ];
    };
}

registry! {
    NewProject => ("project-new", Project, "new project", "novo projeto"),
    OpenWelcome => ("shell-projects", Project, "recent projects welcome", "projetos recentes início"),
    OpenProject => ("project-open", Project, "open file", "abrir arquivo"),
    SaveProject => ("project-save", Project, "save", "salvar"),
    SaveProjectAs => ("project-save-as", Project, "save as", "salvar como"),
    RenameProject => ("project-rename", Project, "rename project name", "renomear projeto nome"),
    OpenSettings => ("settings-open", Project, "settings preferences units shortcuts", "configurações preferências unidades atalhos"),
    Undo => ("undo", Design, "undo", "desfazer"),
    Redo => ("redo", Design, "redo", "refazer"),
    NewBoard => ("board-new", Design, "new board part", "nova chapa peça"),
    NewMaterial => ("material-new", Design, "new material", "novo material"),
    EditMaterial => ("material-edit", Design, "edit material", "editar material"),
    EditGrid => ("grid-edit", Design, "grid spacing", "espaçamento grade"),
    EditKerf => ("cutting-kerf-edit", CutPlan, "cut kerf", "espessura corte"),
    AddCatalog => ("catalog-add", Hardware, "add catalog", "adicionar catálogo"),
    UpdateCatalog => ("catalog-update", Hardware, "refresh catalog", "atualizar catálogo"),
    BatchDimensions => ("board-batch-edit", Design, "resize selected boards", "redimensionar chapas"),
    SelectObject => ("assembly-hierarchy", Design, "select hierarchy", "selecionar hierarquia"),
    ToggleVisibility => ("assembly-hide", Design, "hide reveal", "ocultar mostrar"),
    Group => ("assembly-group", Design, "group", "agrupar"),
    Reparent => ("assembly-reparent", Design, "reparent", "mudar pai"),
    Ungroup => ("assembly-ungroup", Design, "ungroup", "desagrupar"),
    DuplicateAssembly => ("assembly-duplicate", Design, "duplicate assembly", "duplicar conjunto"),
    Transform => ("assembly-transform", Design, "transform hierarchy", "transformar hierarquia"),
    SelectBoard => ("board-list", Design, "select board", "selecionar peça"),
    AssignMaterial => ("board-assign-material", Design, "assign material", "atribuir material"),
    PositionBoard => ("placement-numeric", Design, "position board", "posicionar peça"),
    PlaceFace => ("placement-face", Design, "place face to face", "posicionar face a face"),
    DuplicateBoard => ("board-duplicate", Design, "duplicate board", "duplicar peça"),
    RenameObject => ("object-rename", Design, "rename board assembly name", "renomear peça conjunto nome"),
    EditDimensions => ("board-edit-dimension", Design, "edit dimension", "editar dimensão"),
    SetGrain => ("board-grain", Design, "grain direction", "sentido veio"),
    ToggleBanding => ("banding-toggle", Design, "band edge tape edge banding", "fita de borda colocar tirar"),
    SetBanding => ("banding-set", Design, "edge banding automatic set", "fita de borda automática definir"),
    ApplyBandingPreset => ("banding-preset", Design, "edge banding preset all front none automatic", "fita de borda todas frente nenhuma automática"),
    NewEdgeBand => ("edge-band-new", Design, "new edge band tape", "nova fita de borda"),
    EditEdgeBand => ("edge-band-edit", Design, "edit edge band tape", "editar fita de borda"),
    RemoveEdgeBand => ("edge-band-remove", Design, "remove delete edge band tape", "remover excluir fita de borda"),
    NewStock => ("stock-new", Stock, "new stock sheet", "nova chapa estoque"),
    EditStock => ("stock-edit", Stock, "edit stock", "editar estoque"),
    DuplicateStock => ("stock-duplicate", Stock, "duplicate copy sheet offcut stock", "duplicar copiar chapa sobra estoque"),
    DeleteStock => ("stock-delete", Stock, "delete remove sheet offcut stock", "excluir remover chapa sobra estoque"),
    StockMove => ("stock-priority-view", Stock, "move stock to visible or global rank", "mover chapa para posição visível ou global"),
    EditCutFee => ("cut-fee-edit", Stock, "cutting fee", "custo corte"),
    EditCurrency => ("currency-change-heading", Stock, "change project currency relabel replacement", "alterar moeda do projeto substituir preços"),
    NewHardware => ("hardware-new", Hardware, "new hardware", "nova ferragem"),
    EditHardware => ("hardware-edit", Hardware, "edit hardware", "editar ferragem"),
    DuplicateHardware => ("hardware-duplicate", Hardware, "duplicate hardware", "duplicar ferragem"),
    DeleteHardware => ("hardware-remove", Hardware, "remove reference hardware", "remover ferragem de referência"),
    NewHinge => ("hinge-new", Hardware, "new hinge", "nova dobradiça"),
    EditHinge => ("hinge-edit", Hardware, "edit hinge", "editar dobradiça"),
    DeleteHinge => ("hinge-delete", Hardware, "delete hinge", "excluir dobradiça"),
    NewDoor => ("door-add", Hardware, "add door relationship", "adicionar porta"),
    EditDoor => ("door-edit", Hardware, "edit door", "editar porta"),
    DeleteDoor => ("door-delete", Hardware, "remove door relationship", "remover porta"),
    NewSlides => ("slide-new", Hardware, "add drawer slides", "adicionar corrediças gaveta"),
    EditSlides => ("slide-edit", Hardware, "edit drawer slides length", "editar corrediças comprimento"),
    DeleteSlides => ("slide-delete", Hardware, "remove drawer slides", "remover corrediças"),
    NewFoot => ("foot-new", Hardware, "add foot leg", "adicionar pé"),
    RefitSlides => ("slide-refit", Hardware, "refit redetect drawer slides", "reajustar corrediças"),
    RemoveCatalog => ("catalog-remove", Hardware, "remove unpin catalog model", "remover modelo catálogo"),
    DeleteObject => ("door-delete-object", Design, "delete selected object", "excluir objeto"),
    StartMotion => ("door-motion-start", Hardware, "preview door motion", "prévia movimento porta"),
    CloseMotion => ("door-motion-exit", Hardware, "close door preview", "fechar prévia porta"),
    LocateIssue => ("global-locate", CutPlan, "locate allocation issue", "localizar problema alocação"),
    RepairIssue => ("global-repair", CutPlan, "repair allocation", "reparar alocação"),
    AddIssueStock => ("stock-new", CutPlan, "add stock for issue", "adicionar estoque problema"),
    BeginRepair => ("sheet-edit", CutPlan, "begin sheet repair", "iniciar reparo plano"),
    AcceptRepair => ("sheet-accept", CutPlan, "accept repair", "aceitar reparo"),
    CancelRepair => ("sheet-cancel", CutPlan, "cancel repair", "cancelar reparo"),
    StageRepair => ("sheet-stage", CutPlan, "stage placement transfer rotate", "preparar posição transferência rotação"),
    Unallocate => ("sheet-unallocate-action", CutPlan, "unallocate part", "desalocar peça"),
    ToggleAllocationLock => ("sheet-lock", CutPlan, "lock unlock part", "travar destravar peça"),
    SelectSheetBoard => ("board-list", CutPlan, "select sheet part", "selecionar peça plano"),
    PlaceUnallocated => ("sheet-place-unallocated", CutPlan, "place unallocated parts automatically allocate nest", "posicionar peças sem chapa automaticamente alocar"),
    ReplanSheets => ("sheet-replan", CutPlan, "re-plan all sheets repack nest", "refazer plano chapas reorganizar"),
    AddSuggestedSheets => ("sheet-add-suggested", CutPlan, "add missing sheets", "adicionar chapas que faltam"),
    StartOptimization => ("optimize-start", CutPlan, "optimize layout", "otimizar plano"),
    CancelOptimization => ("optimize-cancel", CutPlan, "cancel optimizer", "cancelar otimizador"),
    AcceptOptimization => ("optimize-accept", CutPlan, "accept optimized layout", "aceitar plano otimizado"),
    ConfirmKerf => ("export-confirm-kerf", Handoff, "confirm cutting kerf", "confirmar espessura corte"),
    OpenHandoff => ("shell-export", Handoff, "review export handoff", "revisar exportação entrega"),
    ExportPdf => ("export-choose", Handoff, "export pdf", "exportar pdf"),
    ReplacePdf => ("export-replace", Dialog, "replace pdf", "substituir pdf"),
    CancelExport => ("cancel", Dialog, "cancel export", "cancelar exportação"),
    DirtySave => ("project-save", Dialog, "save unsaved changes", "salvar alterações"),
    DirtyDiscard => ("project-discard", Dialog, "discard changes", "descartar alterações"),
    OverwriteProject => ("project-replace", Dialog, "replace project file", "substituir projeto"),
    Recover => ("project-recover", Dialog, "recover snapshot", "recuperar cópia"),
    DiscardRecovery => ("project-recovery-discard", Dialog, "discard recovery", "descartar recuperação"),
    DeferRecovery => ("project-defer", Dialog, "decide later", "decidir depois"),
    ConfirmDialog => ("confirm", Dialog, "confirm apply create save", "confirmar aplicar criar salvar"),
    CancelDialog => ("cancel", Dialog, "cancel close", "cancelar fechar"),
    SetUiLanguage => ("ui-language", Design, "interface language", "idioma interface"),
    SetMeasurementScope => ("measurement-selection", Design, "measure body overall", "medir corpo total"),
    SetMeasurementFrame => ("measurement-frame", Design, "measurement coordinates", "coordenadas medição"),
    SetExportMode => ("export-preparation", Handoff, "draft shop ready", "rascunho pronto oficina"),
    SetExportLanguage => ("export-language", Handoff, "pdf language", "idioma pdf"),
    SetExportUnits => ("export-output-units", Handoff, "pdf units", "unidades pdf"),
    SetOptimizerObjective => ("optimize-heading", CutPlan, "optimization objective", "objetivo otimização"),
    SetDoorAngle => ("door-motion-angle", Hardware, "door preview angle", "ângulo prévia porta"),
    ViewNavigate => ("viewport-navigate", Design, "navigate orbit camera", "navegar orbitar câmera"),
    ViewMove => ("viewport-move", Design, "move board tool", "mover peça ferramenta"),
    ViewMeasure => ("viewport-measure", Design, "measure bounding dimensions", "medir dimensões envolventes"),
    ViewBand => ("viewport-band", Design, "edge banding tool tape edges", "ferramenta fita de borda bordas"),
    ViewFrame => ("viewport-frame", Design, "frame selection camera", "enquadrar seleção câmera"),
    ViewPreset => ("viewport-preset", Design, "isometric front right top camera", "isométrica frontal direita superior câmera"),
    ViewProjection => ("viewport-projection", Design, "perspective orthographic camera", "perspectiva ortográfica câmera"),
}

/// Shared guard for both the viewport surface and future shortcut/palette routes.
pub(crate) fn viewport_availability(
    request: Request,
    modal: bool,
    preview_active: bool,
    dragging: bool,
    project: &Project,
    selection: &viewport::Selection,
) -> Result<(), Unavailable> {
    use ActionId as A;
    if modal {
        return Err(Unavailable::ModalOpen);
    }
    match request.id {
        A::ViewNavigate | A::ViewMove | A::ViewMeasure | A::ViewBand
            if preview_active || dragging =>
        {
            Err(Unavailable::Busy)
        }
        A::ViewFrame | A::ViewPreset | A::ViewProjection if dragging => Err(Unavailable::Busy),
        A::ViewFrame
            if !selection.ids.is_empty() && !viewport::has_frame_bounds(project, selection) =>
        {
            Err(Unavailable::NoSelection)
        }
        A::ViewPreset if !matches!(request.argument, Argument::Preset(preset) if preset != viewport::Preset::Free) => {
            Err(Unavailable::MissingTarget)
        }
        A::ViewProjection if !matches!(request.argument, Argument::Projection(_)) => {
            Err(Unavailable::MissingTarget)
        }
        A::ViewNavigate
        | A::ViewMove
        | A::ViewMeasure
        | A::ViewBand
        | A::ViewFrame
        | A::ViewPreset
        | A::ViewProjection => Ok(()),
        _ => Err(Unavailable::MissingTarget),
    }
}

pub(crate) fn viewport_control(
    request: Request,
    camera: &mut viewport::Camera,
    tool: &mut viewport::MoveTool,
    project: &Project,
    selection: &viewport::Selection,
    modal: bool,
    preview_active: bool,
) -> Result<(), Unavailable> {
    viewport_availability(
        request,
        modal,
        preview_active,
        tool.dragging(),
        project,
        selection,
    )?;
    viewport::apply_control(request, camera, tool, project, selection);
    Ok(())
}

impl ActionId {
    pub(crate) fn descriptor(self) -> &'static Descriptor {
        ALL.iter()
            .find(|entry| entry.id == self)
            .expect("registered action")
    }

    pub(crate) fn label(self, localizer: &Localizer) -> String {
        localizer.text(self.descriptor().key)
    }

    /// The workspace or surface this action belongs to, for display.
    pub(crate) fn route_name(self, language: Language) -> &'static str {
        self.descriptor().route.name(language)
    }

    pub(crate) fn keywords(self, language: Language) -> &'static str {
        let entry = self.descriptor();
        match language {
            Language::En => entry.en_keywords,
            Language::PtBr => entry.pt_keywords,
        }
    }
}

pub(crate) fn button(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    request: Request,
    availability: Result<(), Unavailable>,
) -> egui::Response {
    let entry = request.id.descriptor();
    let response = ui.add_enabled(
        availability.is_ok(),
        egui::Button::new(request.id.label(localizer)),
    );
    match availability {
        Ok(()) => response.on_hover_text(format!(
            "{} · {} · {}",
            entry.route.name(localizer.language()),
            request.id.keywords(localizer.language()),
            entry.stable_id,
        )),
        Err(reason) => response.on_disabled_hover_text(reason.reason(localizer.language())),
    }
}

/// Form and session controllers retain their draft, while sharing this typed
/// invocation gate and its explicit unavailability with other action routes.
pub(crate) fn contextual<T>(
    request: Request,
    availability: Result<(), Unavailable>,
    action: impl FnOnce() -> T,
) -> Result<T, Unavailable> {
    let _ = request.id.descriptor();
    availability.map(|()| action())
}

pub(crate) fn decision(id: ActionId, clicked: bool) -> bool {
    clicked && contextual(Request::new(id), Ok(()), || ()).is_ok()
}

/// Project shortcuts never run while text or a modal/popup owns the keyboard.
/// Their eventual invocation still passes through the action and draft guards.
pub(crate) fn project_shortcut(ctx: &egui::Context, blocked: bool) -> Option<ActionId> {
    if blocked || ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) {
        return None;
    }
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && !modifiers.alt => match (key, modifiers.shift) {
                (egui::Key::S, false) => Some(ActionId::SaveProject),
                (egui::Key::Comma, false) => Some(ActionId::OpenSettings),
                (egui::Key::Z, false) => Some(ActionId::Undo),
                (egui::Key::Z, true) => Some(ActionId::Redo),
                _ => None,
            },
            _ => None,
        })
    })
}

pub(crate) fn select_sheet_board(
    project: &Project,
    selection: &mut viewport::Selection,
    request: Request,
) -> Result<(), Unavailable> {
    if request.id != ActionId::SelectSheetBoard {
        return Err(Unavailable::MissingTarget);
    }
    let Target::Board(id) = request.target else {
        return Err(Unavailable::MissingTarget);
    };
    if !project.boards.iter().any(|b| b.id == id) {
        return Err(Unavailable::MissingTarget);
    }
    selection.choose(
        Some(id),
        matches!(request.argument, Argument::Additive(true)),
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    None,
    Object(Uuid),
    Board(Uuid),
    Material(Uuid),
    Stock(Uuid),
    Catalog(Uuid),
    Hinge(Uuid),
    Door(Uuid),
    /// A drawer's slide pair.
    Slide(Uuid),
    /// An edge band record.
    Band(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Request {
    pub id: ActionId,
    pub target: Target,
    pub argument: Argument,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Argument {
    None,
    Additive(bool),
    Grain(Option<BoardGrain>),
    Language(Language),
    Scope(Scope),
    Frame(Frame),
    ExportMode(ExportMode),
    Unit(Unit),
    Angle(f64),
    Preset(viewport::Preset),
    Projection(viewport::Projection),
    StockPriority {
        target: usize,
        subset: bool,
    },
    Length(plan_my_cabinet::units::Length),
    Edge(plan_my_cabinet::domain::BoardEdge),
    EdgeSetting {
        edge: plan_my_cabinet::domain::BoardEdge,
        value: plan_my_cabinet::domain::EdgeBanding,
    },
    BandingPreset(plan_my_cabinet::banding::BandingPreset),
}

impl Request {
    pub(crate) fn new(id: ActionId) -> Self {
        Self {
            id,
            target: Target::None,
            argument: Argument::None,
        }
    }
    pub(crate) fn with(id: ActionId, target: Target) -> Self {
        Self {
            id,
            target,
            argument: Argument::None,
        }
    }
    pub(crate) fn argument(mut self, argument: Argument) -> Self {
        self.argument = argument;
        self
    }
}

fn stock_move_target_valid(project: &Project, request: Request) -> bool {
    let (Target::Stock(id), Argument::StockPriority { target, subset }) =
        (request.target, request.argument)
    else {
        return false;
    };
    let Some(piece) = project.stock.iter().find(|piece| piece.id == id) else {
        return false;
    };
    target
        < if subset {
            project
                .stock
                .iter()
                .filter(|row| row.material_id == piece.material_id)
                .count()
        } else {
            project.stock.len()
        }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    ModalOpen,
    PendingEdit,
    Busy,
    NoUndo,
    NoRedo,
    NoSelection,
    MissingTarget,
    NeedsMaterial,
    NeedsAnotherBoard,
    InvalidHardware,
    InvalidMotion,
    NoRepair,
    NoOptimization,
    StaleOptimization,
    AlreadyConfirmed,
    ExportNotReady,
    NoDialog,
    StockInUse,
    CatalogInUse,
    BandInUse,
    NeedsBand,
    NoBanding,
}

impl Unavailable {
    pub(crate) fn reason(self, language: Language) -> &'static str {
        match (language, self) {
            (Language::En, Self::ModalOpen) => "Finish or cancel the active dialog first",
            (Language::PtBr, Self::ModalOpen) => "Conclua ou cancele a janela aberta primeiro",
            (Language::En, Self::PendingEdit) => {
                "Resolve the unfinished edit before running this command"
            }
            (Language::PtBr, Self::PendingEdit) => {
                "Resolva a edição pendente antes de executar este comando"
            }
            (Language::En, Self::Busy) => "Wait for the current file or export operation",
            (Language::PtBr, Self::Busy) => "Aguarde a operação de arquivo ou exportação",
            (Language::En, Self::NoUndo) => "Nothing to undo",
            (Language::PtBr, Self::NoUndo) => "Nada para desfazer",
            (Language::En, Self::NoRedo) => "Nothing to redo",
            (Language::PtBr, Self::NoRedo) => "Nada para refazer",
            (Language::En, Self::BandInUse) => {
                "Boards or materials still use this band; change them first"
            }
            (Language::PtBr, Self::BandInUse) => {
                "Peças ou materiais ainda usam esta fita; altere-os primeiro"
            }
            (Language::En, Self::NeedsBand) => "Create an edge band first",
            (Language::PtBr, Self::NeedsBand) => "Crie uma fita de borda primeiro",
            (Language::En, Self::NoBanding) => {
                "This board's material takes no edge banding (only MDF and MDP do)"
            }
            (Language::PtBr, Self::NoBanding) => {
                "O material desta peça não leva fita de borda (só MDF e MDP levam)"
            }
            (Language::En, Self::CatalogInUse) => {
                "Hinges, slides or feet still use this model; remove them first"
            }
            (Language::PtBr, Self::CatalogInUse) => {
                "Dobradiças, corrediças ou pés ainda usam este modelo; remova-os primeiro"
            }
            (Language::En, Self::StockInUse) => {
                "Parts are placed on this piece; move or unallocate them first"
            }
            (Language::PtBr, Self::StockInUse) => {
                "Há peças nesta chapa; mova ou desaloque-as primeiro"
            }
            (Language::En, Self::NoSelection) => "Select an object first",
            (Language::PtBr, Self::NoSelection) => "Selecione um objeto primeiro",
            (Language::En, Self::MissingTarget) => "The target no longer exists",
            (Language::PtBr, Self::MissingTarget) => "O destino não existe mais",
            (Language::En, Self::NeedsMaterial) => "Create a material first",
            (Language::PtBr, Self::NeedsMaterial) => "Crie um material primeiro",
            (Language::En, Self::NeedsAnotherBoard) => "Add another board first",
            (Language::PtBr, Self::NeedsAnotherBoard) => "Adicione outra peça primeiro",
            (Language::En, Self::InvalidHardware) => {
                "Only dimensioned reference hardware can be edited or duplicated"
            }
            (Language::PtBr, Self::InvalidHardware) => {
                "Somente ferragem de referência dimensionada pode ser editada ou duplicada"
            }
            (Language::En, Self::InvalidMotion) => {
                "This door cannot be previewed until its relationship is valid"
            }
            (Language::PtBr, Self::InvalidMotion) => {
                "Esta porta precisa de relação válida para prévia"
            }
            (Language::En, Self::NoRepair) => "Start a sheet repair first",
            (Language::PtBr, Self::NoRepair) => "Inicie o reparo da chapa primeiro",
            (Language::En, Self::NoOptimization) => "Start or complete optimization first",
            (Language::PtBr, Self::NoOptimization) => "Inicie ou conclua a otimização primeiro",
            (Language::En, Self::StaleOptimization) => {
                "Manufacturing inputs changed; start a fresh search"
            }
            (Language::PtBr, Self::StaleOptimization) => {
                "Os dados de fabricação mudaram; inicie uma nova busca"
            }
            (Language::En, Self::AlreadyConfirmed) => "The current kerf is already confirmed",
            (Language::PtBr, Self::AlreadyConfirmed) => {
                "A espessura de corte atual já está confirmada"
            }
            (Language::En, Self::ExportNotReady) => "Prepare a valid packet before exporting",
            (Language::PtBr, Self::ExportNotReady) => "Prepare um pacote válido antes de exportar",
            (Language::En, Self::NoDialog) => "Open the corresponding dialog first",
            (Language::PtBr, Self::NoDialog) => "Abra a janela correspondente primeiro",
        }
    }
}

impl DesktopApp {
    /// One availability boundary for palette, shortcuts, and visible controls.
    pub(crate) fn action_availability(&self, request: Request) -> Result<(), Unavailable> {
        use ActionId as A;
        use Target as T;
        let id = request.id;
        let project = self.editor.project();
        if matches!(
            id,
            A::ViewNavigate
                | A::ViewMove
                | A::ViewMeasure
                | A::ViewFrame
                | A::ViewPreset
                | A::ViewProjection
        ) {
            return viewport_availability(
                request,
                self.modal_open() || self.palette.open,
                self.editor.preview().is_some(),
                self.design.move_tool.dragging(),
                project,
                &self.selection,
            );
        }
        // Palette dispatch is the same command as a visible control; its own
        // modal layer must not make every listed action unavailable.
        let modal = self.other_modal_open()
            || self.cut_plan.optimizer.comparison_open()
            || self.cut_plan.repair.active()
            || self.hardware.door_motion.is_some()
            || self.project_files.blocking();
        if id == A::OpenHandoff {
            return if self.project_files.blocking() || self.cut_plan.optimizer.comparison_open() {
                Err(Unavailable::Busy)
            } else if self.navigation.pending().is_some()
                || self.other_modal_open()
                    && self.modals.board_dimension().is_none()
                    && self.modals.placement().is_none()
            {
                Err(Unavailable::ModalOpen)
            } else {
                Ok(())
            };
        }
        if id == A::RepairIssue
            && (self.other_modal_open()
                || self.project_files.blocking()
                || self.hardware.door_motion.is_some())
        {
            return Err(Unavailable::ModalOpen);
        }
        if matches!(
            id,
            A::NewProject | A::OpenProject | A::SaveProject | A::SaveProjectAs | A::OpenWelcome
        ) {
            // Saving writes the committed project, so door motion (a view
            // angle) and a background search (its own snapshot) do not block
            // it. Replacing the project cancels a search but not a motion mode.
            let saving = matches!(id, A::SaveProject | A::SaveProjectAs);
            if (self.other_modal_open()
                && self.modals.placement().is_none()
                && self.modals.board_dimension().is_none())
                || self.cut_plan.optimizer.comparison_open()
                || self.navigation.pending().is_some()
                || (self.hardware.door_motion.is_some() && !saving)
                || self.project_files.blocking()
                || self.handoff.activity.is_some()
            {
                return Err(Unavailable::Busy);
            }
        } else if modal
            && !matches!(
                id,
                A::CloseMotion
                    | A::SetDoorAngle
                    | A::RepairIssue
                    | A::CancelRepair
                    | A::AcceptRepair
                    | A::StageRepair
                    | A::Unallocate
                    | A::ToggleAllocationLock
                    | A::CancelOptimization
                    | A::CancelExport
                    | A::ReplacePdf
                    | A::DirtySave
                    | A::DirtyDiscard
                    | A::OverwriteProject
                    | A::Recover
                    | A::DiscardRecovery
                    | A::DeferRecovery
            )
        {
            return Err(Unavailable::ModalOpen);
        }
        match id {
            A::ConfirmDialog
            | A::CancelDialog
            | A::DirtySave
            | A::DirtyDiscard
            | A::OverwriteProject
            | A::Recover
            | A::DiscardRecovery
            | A::DeferRecovery
            | A::ReplacePdf
            | A::CancelExport
                if !modal && self.handoff.activity.is_none() =>
            {
                Err(Unavailable::NoDialog)
            }
            A::SetGrain if !matches!(request.argument, Argument::Grain(_)) => {
                Err(Unavailable::MissingTarget)
            }
            A::ToggleBanding if !matches!((request.target, request.argument), (T::Board(id), Argument::Edge(_)) if project.boards.iter().any(|b| b.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::ToggleBanding | A::SetBanding | A::ApplyBandingPreset
                if !self.banding_targets(request.target).iter().any(|id| {
                    project
                        .board(*id)
                        .and_then(|b| project.material(b.material_id))
                        .is_some_and(|m| m.kind.accepts_banding())
                }) =>
            {
                if self.banding_targets(request.target).is_empty() {
                    Err(Unavailable::NoSelection)
                } else {
                    Err(Unavailable::NoBanding)
                }
            }
            A::ToggleBanding if matches!(request.target, T::Board(id) if self.click_band(id).is_none()) => {
                Err(Unavailable::NeedsBand)
            }
            A::SetBanding if !matches!(request.argument, Argument::EdgeSetting { .. }) => {
                Err(Unavailable::MissingTarget)
            }
            A::ApplyBandingPreset if !matches!(request.argument, Argument::BandingPreset(_)) => {
                Err(Unavailable::MissingTarget)
            }
            A::EditEdgeBand | A::RemoveEdgeBand if !matches!(request.target, T::Band(id) if project.edge_band(id).is_some()) => {
                Err(Unavailable::MissingTarget)
            }
            A::RemoveEdgeBand if matches!(request.target, T::Band(id) if { let (b, m) = plan_my_cabinet::banding::band_usage(project, id); !b.is_empty() || !m.is_empty() }) => {
                Err(Unavailable::BandInUse)
            }
            A::SetMeasurementFrame if matches!(request.argument, Argument::Frame(Frame::Object(target)) if !project.boards.iter().any(|b| b.id == target) && !project.assemblies.iter().any(|a| a.id == target)) => {
                Err(Unavailable::MissingTarget)
            }
            A::SetDoorAngle if !matches!(request.target, T::Door(id) | T::Slide(id) if self.hardware.door_motion.is_some_and(|(active, _)| active == id)) => {
                Err(Unavailable::InvalidMotion)
            }
            A::Undo if !self.editor.can_undo() => Err(Unavailable::NoUndo),
            A::Redo if !self.editor.can_redo() => Err(Unavailable::NoRedo),
            A::Group | A::Reparent | A::Transform if self.selection.ids.is_empty() => {
                Err(Unavailable::NoSelection)
            }
            A::BatchDimensions
                if !self
                    .selection
                    .ids
                    .iter()
                    .any(|id| project.boards.iter().any(|b| b.id == *id)) =>
            {
                Err(Unavailable::NoSelection)
            }
            A::DeleteObject
                if !self.selection.active.is_some_and(|id| {
                    project.boards.iter().any(|b| b.id == id)
                        || project.assemblies.iter().any(|a| a.id == id)
                }) =>
            {
                Err(Unavailable::NoSelection)
            }
            A::Ungroup | A::DuplicateAssembly
                if !self
                    .selection
                    .active
                    .is_some_and(|id| project.assemblies.iter().any(|a| a.id == id)) =>
            {
                Err(Unavailable::NoSelection)
            }
            A::AssignMaterial if project.materials.is_empty() => Err(Unavailable::NeedsMaterial),
            A::PlaceFace if project.boards.len() < 2 => Err(Unavailable::NeedsAnotherBoard),
            A::EditMaterial if !matches!(request.target, T::Material(id) if project.materials.iter().any(|m| m.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::NewStock if matches!(request.target, T::Material(id) if !project.materials.iter().any(|m| m.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::RenameObject if !matches!(request.target, T::Object(id) if project.boards.iter().any(|b| b.id == id) || project.assemblies.iter().any(|a| a.id == id) || project.hardware.iter().any(|h| h.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::SelectObject | A::ToggleVisibility if !matches!(request.target, T::Object(id) if project.boards.iter().any(|b| b.id == id) || project.assemblies.iter().any(|a| a.id == id) || project.hardware.iter().any(|h| h.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::SelectBoard
            | A::AssignMaterial
            | A::PositionBoard
            | A::PlaceFace
            | A::DuplicateBoard
            | A::EditDimensions
            | A::SetGrain
            | A::LocateIssue
            | A::RepairIssue
            | A::AddIssueStock
                if !matches!(request.target, T::Board(id) if project.boards.iter().any(|b| b.id == id)) =>
            {
                Err(Unavailable::MissingTarget)
            }
            A::EditStock | A::StockMove | A::DuplicateStock | A::DeleteStock if !matches!(request.target, T::Stock(id) if project.stock.iter().any(|s| s.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::DeleteStock if matches!(request.target, T::Stock(id) if project.allocations.iter().any(|a| a.stock_id == id)) => {
                Err(Unavailable::StockInUse)
            }
            A::StockMove if !stock_move_target_valid(project, request) => {
                Err(Unavailable::MissingTarget)
            }
            A::UpdateCatalog if !matches!(request.target, T::Catalog(id) if project.catalog.iter().any(|c| c.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::EditHardware | A::DuplicateHardware | A::DeleteHardware if !matches!(request.target, T::Object(id) if project.hardware.iter().any(|h| h.id == id && (matches!(h.kind, plan_my_cabinet::domain::HardwareKind::Placeholder { .. }) || project.foot_spec(h).is_some()))) => {
                Err(Unavailable::InvalidHardware)
            }
            A::EditHinge | A::DeleteHinge if !matches!(request.target, T::Hinge(id) if project.hinge_installations.iter().any(|h| h.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::EditDoor | A::DeleteDoor if !matches!(request.target, T::Door(id) if project.door_joints.iter().any(|j| j.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::RemoveCatalog if !matches!(request.target, T::Catalog(id) if project.catalog.iter().any(|c| c.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::RemoveCatalog if matches!(request.target, T::Catalog(id) if !plan_my_cabinet::hardware_catalog::usage(project, id).is_empty()) => {
                Err(Unavailable::CatalogInUse)
            }
            A::EditSlides | A::DeleteSlides | A::RefitSlides if !matches!(request.target, T::Slide(id) if project.slide_installations.iter().any(|s| s.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::StartMotion if !matches!(request.target, T::Door(id) | T::Slide(id) if door_joint_ui::motion_limit(project, id).is_some()) => {
                Err(Unavailable::InvalidMotion)
            }
            A::CloseMotion if self.hardware.door_motion.is_none() => {
                Err(Unavailable::InvalidMotion)
            }
            A::AcceptRepair
            | A::CancelRepair
            | A::StageRepair
            | A::Unallocate
            | A::ToggleAllocationLock
                if !self.cut_plan.repair.active() =>
            {
                Err(Unavailable::NoRepair)
            }
            A::BeginRepair if self.cut_plan.repair.active() => Err(Unavailable::NoRepair),
            A::PlaceUnallocated | A::ReplanSheets | A::AddSuggestedSheets
                if self.cut_plan.repair.active() || self.editor.preview().is_some() =>
            {
                Err(Unavailable::Busy)
            }
            A::AddSuggestedSheets if !matches!((request.target, request.argument), (T::Material(id), Argument::Length(_)) if project.materials.iter().any(|m| m.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            A::CancelOptimization if !self.cut_plan.optimizer.running() => {
                Err(Unavailable::NoOptimization)
            }
            A::StartOptimization | A::SetOptimizerObjective
                if self.cut_plan.optimizer.running() || self.editor.preview().is_some() =>
            {
                Err(Unavailable::Busy)
            }
            A::AcceptOptimization => self.cut_plan.optimizer.acceptance_availability(project),
            A::ConfirmKerf if project.confirmed_shop_kerf == Some(project.cutting_kerf) => {
                Err(Unavailable::AlreadyConfirmed)
            }
            A::SetExportMode
                if matches!(
                    request.argument,
                    Argument::ExportMode(ExportMode::ShopReady)
                ) && !self.shop_ready_available() =>
            {
                Err(Unavailable::ExportNotReady)
            }
            A::ExportPdf if self.handoff.activity.is_some() => Err(Unavailable::Busy),
            A::ExportPdf if self.current_reviewed_packet().is_none() => {
                Err(Unavailable::ExportNotReady)
            }
            A::SelectSheetBoard if !matches!(request.target, T::Board(id) if project.boards.iter().any(|b| b.id == id)) => {
                Err(Unavailable::MissingTarget)
            }
            _ => Ok(()),
        }
    }

    /// UI entry point for clicks and shortcuts: a refused action becomes a
    /// toast with its reason instead of silently doing nothing.
    pub(crate) fn invoke_or_report(&mut self, request: Request) {
        if let Err(reason) = self.invoke(request) {
            self.report_unavailable(reason);
        }
    }

    pub(crate) fn report_unavailable(&mut self, reason: Unavailable) {
        // An empty history is the expected no-op of Cmd+Z, not a failure.
        if !matches!(reason, Unavailable::NoUndo | Unavailable::NoRedo) {
            self.toasts.error(reason.reason(self.localizer.language()));
        }
    }

    /// One toast for an automatic placement: what moved, what was added and
    /// what is still waiting (the Needs stock list says why).
    pub(crate) fn report_placement(
        &mut self,
        result: Result<
            PlaceSummary,
            plan_my_cabinet::commands::EditError<std::convert::Infallible>,
        >,
    ) {
        let summary = match result {
            Ok(summary) => summary,
            Err(error) => return self.report_edit(Err::<(), _>(error)),
        };
        self.cut_plan.material_conflicts = allocation_conflicts(self.editor.project());
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("placed", summary.placed);
        args.set("sheets", summary.sheets_added);
        args.set("waiting", summary.unplaced.len());
        let mut text = self.localizer.format(
            match (summary.sheets_added, summary.placed) {
                (0, 0) => "toast-place-nothing",
                (0, _) => "toast-placed",
                _ => "toast-sheets-added",
            },
            Some(&args),
        );
        if !summary.unplaced.is_empty() {
            text.push(' ');
            text.push_str(&self.localizer.format("toast-place-waiting", Some(&args)));
            self.toasts.error(text);
        } else {
            self.toasts.info(text);
        }
    }

    /// Edits are atomic, so a rejected one changed nothing; say so.
    pub(crate) fn report_edit<T, E>(
        &mut self,
        result: Result<T, plan_my_cabinet::commands::EditError<E>>,
    ) {
        if result.is_err() {
            self.toasts
                .error(self.localizer.text("toast-edit-rejected"));
        }
    }

    /// Revalidate after a UI click or palette selection, never trust stale IDs.
    pub(crate) fn invoke(&mut self, request: Request) -> Result<(), Unavailable> {
        use ActionId as A;
        use Target as T;
        self.action_availability(request)?;
        if !matches!(
            request.id,
            A::SelectObject
                | A::SelectBoard
                | A::SelectSheetBoard
                | A::ViewNavigate
                | A::ViewMove
                | A::ViewMeasure
                | A::ViewBand
                | A::ViewFrame
                | A::ViewPreset
                | A::ViewProjection
                | A::OpenHandoff
                | A::OpenSettings
                | A::SetUiLanguage
                | A::SetMeasurementScope
                | A::SetMeasurementFrame
                | A::SetExportMode
                | A::SetExportLanguage
                | A::SetExportUnits
                | A::ToggleVisibility
        ) && self.resolve_draft_before_action(request)?
        {
            return Ok(());
        }
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        match (request.id, request.target) {
            (A::NewProject, _) => self.request_project_action(project_ui::NextAction::New),
            (A::RenameProject, _) => {
                self.modals
                    .set_project_name(Some(project_name_ui::ProjectNameDialog::new(
                        project_name_ui::NameMode::Rename,
                        self.editor.project().name.clone(),
                        self.editor.project(),
                    )))
            }
            (A::OpenWelcome, _) => self.request_project_action(project_ui::NextAction::Welcome),
            (A::OpenSettings, _) => self.settings.open = true,
            (A::OpenProject, _) => self.request_project_action(project_ui::NextAction::Open),
            (A::SaveProject, _) => self.request_save(false),
            (A::SaveProjectAs, _) => self.request_save(true),
            (A::Undo, _) => {
                let result = self.editor.undo();
                self.report_edit(result);
                self.cut_plan.material_conflicts = allocation_conflicts(self.editor.project());
            }
            (A::Redo, _) => {
                let result = self.editor.redo();
                self.report_edit(result);
                self.cut_plan.material_conflicts = allocation_conflicts(self.editor.project());
            }
            (A::StartOptimization, _) => {
                if !self.cut_plan.optimizer.start(&self.editor, false) {
                    return Err(Unavailable::Busy);
                }
            }
            (A::CancelOptimization, _) => self.cut_plan.optimizer.cancel(),
            (A::PlaceUnallocated | A::ReplanSheets, _) => {
                let mode = if request.id == A::PlaceUnallocated {
                    PackMode::FillGaps
                } else {
                    PackMode::Replan
                };
                let result = self.editor.auto_place(mode);
                self.report_placement(result);
            }
            (A::AddSuggestedSheets, T::Material(id)) => {
                let Argument::Length(thickness) = request.argument else {
                    return Err(Unavailable::MissingTarget);
                };
                let sheet = suggest_sheets(self.editor.project())
                    .into_iter()
                    .find(|s| s.material_id == id && s.thickness == thickness)
                    .and_then(|s| s.sheet)
                    .filter(|sheet| sheet.count > 0)
                    .ok_or(Unavailable::MissingTarget)?;
                let result = self.editor.add_sheets_and_place(id, thickness, &sheet);
                self.report_placement(result);
            }
            (A::SetOptimizerObjective | A::AcceptOptimization, _) => {
                match self.request_navigation(NavigationRoute::Workspace(Workspace::CutPlan)) {
                    Outcome::Navigated | Outcome::Stayed => {
                        if request.id == A::AcceptOptimization {
                            self.cut_plan.optimizer.open_comparison();
                        } else {
                            self.open_drawer = Some(workspace_shell::Drawer::Controls);
                        }
                    }
                    Outcome::Prompt { .. } => return Ok(()),
                    Outcome::Blocked(_) => return Err(Unavailable::PendingEdit),
                }
            }
            (
                A::ViewNavigate
                | A::ViewMove
                | A::ViewMeasure
                | A::ViewFrame
                | A::ViewPreset
                | A::ViewProjection,
                _,
            ) => {
                viewport::apply_control(
                    request,
                    &mut self.camera,
                    &mut self.design.move_tool,
                    self.editor.project(),
                    &self.selection,
                );
            }
            (A::NewBoard, _) => self.modals.set_creation(Some(CreationDialog::board(
                self.editor.project().materials.first().map(|m| m.id),
            ))),
            (A::NewMaterial, _) => self.modals.set_creation(Some(CreationDialog::material())),
            (A::EditGrid, _) => self
                .modals
                .set_grid(Some(GridDialog::open(self.editor.project(), locale))),
            (A::EditKerf, _) => self.modals.set_grid(Some(GridDialog::cutting_kerf(
                self.editor.project(),
                locale,
            ))),
            (A::AddCatalog, _) => self.open_catalog_dialog(),
            (A::UpdateCatalog, T::Catalog(id)) => self.update_catalog_action(id),
            (A::EditMaterial, T::Material(id)) => self.open_material_action(id, locale),
            (A::BatchDimensions, _) => self.open_batch_action(),
            (A::SelectObject, T::Object(id))
            | (A::SelectBoard, T::Board(id))
            | (A::SelectSheetBoard, T::Board(id)) => {
                if matches!(
                    self.request_scene_selection(
                        Some(id),
                        matches!(request.argument, Argument::Additive(true))
                    ),
                    pending_navigation::Outcome::Blocked(_)
                ) {
                    return Err(Unavailable::PendingEdit);
                }
            }
            (A::RenameObject, T::Object(id)) => {
                match self.request_scene_selection(Some(id), false) {
                    pending_navigation::Outcome::Blocked(_) => {
                        return Err(Unavailable::PendingEdit);
                    }
                    pending_navigation::Outcome::Prompt { .. } => {}
                    pending_navigation::Outcome::Navigated
                    | pending_navigation::Outcome::Stayed => {
                        self.design.rename =
                            assembly_ui::RenameDraft::start(self.editor.project(), id);
                    }
                }
            }
            (A::ToggleVisibility, T::Object(id)) => {
                if self.selection.visible(self.editor.project(), id) {
                    self.selection.hidden.insert(id);
                } else {
                    self.selection.reveal(self.editor.project(), id);
                }
            }
            (A::Group | A::Reparent | A::Ungroup | A::DuplicateAssembly | A::Transform, _) => {
                let operation = match request.id {
                    A::Group => assembly_ui::Operation::Group,
                    A::Reparent => assembly_ui::Operation::Reparent,
                    A::Ungroup => assembly_ui::Operation::Ungroup,
                    A::DuplicateAssembly => assembly_ui::Operation::Duplicate,
                    _ => assembly_ui::Operation::Transform,
                };
                self.modals
                    .set_assembly(Some(assembly_ui::AssemblyDialog::new(self, operation)));
            }
            (A::AssignMaterial, T::Board(id)) => {
                let Some(board) = self.editor.project().board(id) else {
                    return Err(Unavailable::MissingTarget);
                };
                self.modals.set_board_material(Some(BoardMaterialDialog {
                    focus_on_open: true,
                    board_id: id,
                    project_id: self.editor.project().id,
                    revision: self.editor.project().revision,
                    material_id: Some(board.material_id),
                    anchor: Anchor::Centre,
                    error: None,
                }));
            }
            (A::PositionBoard, T::Board(id)) => self
                .modals
                .set_placement(PlacementDialog::numeric(self, id)),
            (A::PlaceFace, T::Board(id)) => {
                self.modals.set_placement(PlacementDialog::face(self, id))
            }
            (A::DuplicateBoard, T::Board(id)) => {
                let Some(mut pose) = self.editor.project().board(id).map(|b| b.pose) else {
                    return Err(Unavailable::MissingTarget);
                };
                pose.translation_mm[0] += 25.0;
                match self.editor.duplicate_board_with_fit(id, pose) {
                    Ok((copy, fit)) => {
                        self.design.board_action_error = false;
                        self.design.first_fit_notice = Some(fit);
                        // Show the copy in the inspector, ready to be renamed.
                        let _ = self.request_scene_selection(Some(copy), false);
                    }
                    Err(_) => self.design.board_action_error = true,
                }
            }
            (A::EditDimensions, T::Board(id)) => {
                let Some(board) = self.editor.project().board(id) else {
                    return Err(Unavailable::MissingTarget);
                };
                self.modals.set_board_dimension(Some(BoardDimensionDialog {
                    focus_on_open: true,
                    board_id: id,
                    project_id: self.editor.project().id,
                    revision: self.editor.project().revision,
                    dimension: BoardDimension::Length,
                    value: DimensionDraft {
                        text: format_length(board.length, Unit::Mm, locale, 3),
                        consent: false,
                    },
                    anchor: Anchor::Centre,
                    error: None,
                }));
            }
            (A::SetGrain, T::Board(id)) => {
                if let Argument::Grain(grain) = request.argument
                    && self.editor.set_board_grain_override(id, grain).is_ok()
                {
                    self.cut_plan.material_conflicts = allocation_conflicts(self.editor.project());
                }
            }
            (A::ToggleBanding, T::Board(id)) => {
                let (Argument::Edge(edge), Some(band)) = (request.argument, self.click_band(id))
                else {
                    return Err(Unavailable::NeedsBand);
                };
                let result = self.editor.toggle_edge_banding(id, edge, band);
                self.report_edit(result);
            }
            (A::SetBanding, target) => {
                if let Argument::EdgeSetting { edge, value } = request.argument {
                    let boards = self.banding_targets(target);
                    let result = self.editor.set_edge_banding(&boards, &[edge], value);
                    self.report_banding(result);
                }
            }
            (A::ApplyBandingPreset, target) => {
                if let Argument::BandingPreset(preset) = request.argument {
                    let boards = self.banding_targets(target);
                    let band = boards.first().and_then(|id| self.click_band(*id));
                    if band.is_none()
                        && matches!(
                            preset,
                            plan_my_cabinet::banding::BandingPreset::Front
                                | plan_my_cabinet::banding::BandingPreset::AllFour
                        )
                    {
                        return Err(Unavailable::NeedsBand);
                    }
                    let result = self.editor.apply_banding_preset(&boards, preset, band);
                    self.report_banding(result);
                }
            }
            (A::NewEdgeBand, _) => self
                .modals
                .set_edge_band(Some(banding_ui::EdgeBandDialog::new(locale))),
            (A::EditEdgeBand, T::Band(id)) => {
                let Some(band) = self.editor.project().edge_band(id) else {
                    return Err(Unavailable::MissingTarget);
                };
                self.modals
                    .set_edge_band(Some(banding_ui::EdgeBandDialog::edit(band, locale)));
            }
            (A::RemoveEdgeBand, T::Band(id)) => {
                let result = self.editor.remove_edge_band(id);
                if result.is_ok() && self.design.banding_band == Some(id) {
                    self.design.banding_band = None;
                }
                self.report_edit(result);
            }
            (A::NewStock, T::Material(id)) => {
                self.modals
                    .set_stock(Some(stock_ui::StockDialog::new_for_material(
                        self.editor.project(),
                        id,
                    )));
            }
            (A::AddIssueStock, T::Board(id)) => {
                self.modals
                    .set_stock(Some(stock_ui::StockDialog::new_for_issue(
                        self.editor.project(),
                        id,
                    )));
            }
            (A::NewStock | A::AddIssueStock, _) => self
                .modals
                .set_stock(Some(stock_ui::StockDialog::new(self.editor.project()))),
            (A::DuplicateStock, T::Stock(id)) => {
                if let Ok(new) = self.editor.duplicate_stock(id) {
                    self.session.stock_piece = Some(new);
                }
            }
            (A::DeleteStock, T::Stock(id)) => {
                if self.editor.delete_stock(id).is_ok() && self.session.stock_piece == Some(id) {
                    self.session.stock_piece = None;
                }
            }
            (A::EditStock, T::Stock(id)) => {
                let Some(stock) = self.editor.project().stock_piece(id) else {
                    return Err(Unavailable::MissingTarget);
                };
                self.modals.set_stock(Some(stock_ui::StockDialog::edit(
                    self.editor.project(),
                    stock,
                    locale,
                )));
            }
            (A::StockMove, T::Stock(id)) => {
                if let Argument::StockPriority { target, subset } = request.argument {
                    if subset {
                        if let Some(piece) = self
                            .editor
                            .project()
                            .stock
                            .iter()
                            .find(|piece| piece.id == id)
                        {
                            let visible: Vec<_> = self
                                .editor
                                .project()
                                .ordered_stock()
                                .iter()
                                .filter(|row| row.material_id == piece.material_id)
                                .map(|row| row.id)
                                .collect();
                            let result = self.editor.reorder_stock_subset(id, target, &visible);
                            self.report_edit(result);
                        }
                    } else {
                        let result = self.editor.reorder_stock(id, target);
                        self.report_edit(result);
                    }
                }
            }
            (A::EditCutFee, _) => {
                let text = self.editor.project().cut_fee.map_or(String::new(), |fee| {
                    format!("{}.{:02}", fee.minor_units() / 100, fee.minor_units() % 100)
                });
                self.modals
                    .set_cut_fee(Some(stock_ui::CutFeeDialog::new(text)));
            }
            (A::EditCurrency, _) => {
                self.modals
                    .set_currency(Some(currency_ui::CurrencyDialog::new(
                        self.editor.project(),
                    )));
            }
            (A::NewHardware, _) => {
                if !self.add_placeholder_now() {
                    self.modals
                        .set_hardware(Some(hardware_ui::HardwareDialog::new(self, None)));
                }
            }
            (A::EditHardware, T::Object(id)) => {
                self.request_inspect(InspectorTarget::Hardware(id));
            }
            (A::DuplicateHardware, T::Object(id)) => {
                if let Ok(copy) = self.editor.duplicate_placeholder(id) {
                    self.selection.choose(Some(copy), false);
                }
            }
            (A::DeleteHardware, T::Object(id)) => {
                self.modals
                    .set_removal(Some(door_joint_ui::RemovalDialog::new(
                        self,
                        door_joint_ui::DoorRemoval::Hardware(id),
                    )));
            }
            (A::NewHinge, _) => {
                if !self.add_hinge_now() {
                    self.modals
                        .set_hinge(Some(hinge_ui::HingeDialog::new(self, None)));
                }
            }
            (A::EditHinge, T::Hinge(id)) => self
                .modals
                .set_hinge(Some(hinge_ui::HingeDialog::new(self, Some(id)))),
            (A::DeleteHinge, T::Hinge(id)) => {
                let result = plan_my_cabinet::hinge_installation::remove(&mut self.editor, id);
                self.report_edit(result);
            }
            (A::NewDoor, _) => {
                if !self.add_door_now() {
                    self.modals
                        .set_door(Some(door_joint_ui::DoorDialog::new(self, None)));
                }
            }
            (A::EditDoor, T::Door(id)) => {
                self.request_inspect(InspectorTarget::Door(id));
            }
            (A::NewSlides, _) => {
                if !self.add_slides_now() {
                    self.modals
                        .set_slide(Some(slide_ui::SlideDialog::new(self, None)));
                }
            }
            (A::EditSlides, T::Slide(id)) => {
                self.request_inspect(InspectorTarget::Slide(id));
            }
            (A::DeleteSlides, T::Slide(id)) => {
                let result = plan_my_cabinet::slide_installation::remove(&mut self.editor, id);
                if matches!(self.hardware.door_motion, Some((active, _)) if active == id) {
                    self.hardware.door_motion = None;
                }
                self.report_edit(result);
            }
            (A::RefitSlides, T::Slide(id)) => {
                let project = self.editor.project();
                let refitted = project
                    .slide_installations
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or(Unavailable::MissingTarget)
                    .and_then(|s| {
                        plan_my_cabinet::slide_installation::refitted(project, s)
                            .map_err(|_| Unavailable::InvalidHardware)
                    })?;
                let result =
                    plan_my_cabinet::slide_installation::update(&mut self.editor, refitted);
                self.report_edit(result);
            }
            (A::RemoveCatalog, T::Catalog(id)) => {
                let result = plan_my_cabinet::hardware_catalog::remove(&mut self.editor, id);
                if result.is_ok() && self.session.inspector == Some(InspectorTarget::Catalog(id)) {
                    self.session.inspector = None;
                }
                self.report_edit(result);
            }
            (A::NewFoot, _) => {
                if !self.add_foot_now() {
                    // No foot model yet: browse the catalog for one.
                    self.open_catalog_dialog();
                }
            }
            (A::DeleteDoor, T::Door(id)) => {
                self.modals
                    .set_removal(Some(door_joint_ui::RemovalDialog::new(
                        self,
                        door_joint_ui::DoorRemoval::Joint(id),
                    )))
            }
            (A::DeleteObject, _) => {
                if let Some(id) = self.selection.active {
                    self.modals
                        .set_removal(Some(door_joint_ui::RemovalDialog::new(
                            self,
                            door_joint_ui::DoorRemoval::Object(id),
                        )));
                }
            }
            (A::StartMotion, T::Door(id) | T::Slide(id)) => {
                self.editor.cancel_preview();
                self.design.move_tool.cancel();
                self.hardware.door_motion = Some((id, 0.0));
            }
            (A::CloseMotion, _) => self.hardware.door_motion = None,
            (A::SetUiLanguage, _) => {
                if let Argument::Language(language) = request.argument {
                    self.set_ui_language(language);
                } else {
                    return Err(Unavailable::MissingTarget);
                }
            }
            (A::SetMeasurementScope, _) => {
                if let Argument::Scope(scope) = request.argument {
                    self.design.measurement_scope = scope;
                } else {
                    return Err(Unavailable::MissingTarget);
                }
            }
            (A::SetMeasurementFrame, _) => {
                if let Argument::Frame(frame) = request.argument {
                    self.design.measurement_frame = frame;
                } else {
                    return Err(Unavailable::MissingTarget);
                }
            }
            (A::SetExportMode, _) => {
                if let Argument::ExportMode(mode) = request.argument {
                    self.handoff.mode = mode;
                } else {
                    return Err(Unavailable::MissingTarget);
                }
            }
            (A::SetExportLanguage, _) => {
                if let Argument::Language(language) = request.argument {
                    self.handoff.language = language;
                } else {
                    return Err(Unavailable::MissingTarget);
                }
            }
            (A::SetExportUnits, _) => {
                if let Argument::Unit(unit) = request.argument {
                    self.handoff.units = unit;
                } else {
                    return Err(Unavailable::MissingTarget);
                }
            }
            (A::SetDoorAngle, T::Door(id) | T::Slide(id)) => {
                if let Argument::Angle(angle) = request.argument {
                    let limit = door_joint_ui::motion_limit(self.editor.project(), id)
                        .ok_or(Unavailable::InvalidMotion)?;
                    if !angle.is_finite() || !(0.0..=limit).contains(&angle) {
                        return Err(Unavailable::InvalidMotion);
                    }
                    self.hardware.door_motion = Some((id, angle));
                } else {
                    return Err(Unavailable::MissingTarget);
                }
            }
            (A::LocateIssue | A::RepairIssue, T::Board(id)) => {
                self.selection.choose(Some(id), false);
                self.selection.reveal(self.editor.project(), id);
                if request.id == A::RepairIssue {
                    self.cut_plan
                        .repair
                        .begin(&mut self.editor, &self.selection, locale);
                }
            }
            (A::ConfirmKerf, _) => {
                self.modals.set_kerf_confirmation(Some(
                    crate::kerf_confirmation_ui::KerfConfirmation::new(self.editor.project()),
                ));
            }
            (A::OpenHandoff, _) => {
                if matches!(
                    self.request_navigation(NavigationRoute::Workspace(Workspace::Handoff)),
                    Outcome::Blocked(_)
                ) {
                    return Err(Unavailable::ModalOpen);
                }
            }
            (A::ExportPdf, _) => {
                let (tx, rx) = mpsc::channel();
                self.handoff.events = Some(rx);
                self.handoff.activity = Some(ExportActivity::Choosing(
                    Box::new(self.editor.project().clone()),
                    ExportSettings {
                        language: self.handoff.language,
                        units: self.handoff.units,
                    },
                    self.handoff.mode,
                ));
                let picker = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_file_name("workshop.pdf")
                    .save_file();
                std::thread::spawn(move || {
                    let path = pollster::block_on(picker).map(|handle| handle.path().to_path_buf());
                    let _ = tx.send(ExportEvent::Selected(path));
                });
            }
            // The remaining actions operate inside live draft/session controllers.
            // Their controls call through `invoke_contextual` with the same availability boundary.
            _ => return Err(Unavailable::MissingTarget),
        }
        Ok(())
    }

    pub(crate) fn invoke_contextual(&self, request: Request) -> Result<(), Unavailable> {
        self.action_availability(request)
    }

    fn open_material_action(&mut self, id: Uuid, locale: Locale) {
        let Some(material) = self.editor.project().material(id) else {
            return;
        };
        self.modals.set_material_edit(Some(MaterialEditDialog {
            focus_on_open: true,
            id,
            name: material.name.clone(),
            thickness: DimensionDraft {
                text: format_length(material.default_thickness, Unit::Mm, locale, 3),
                consent: false,
            },
            grain: material.default_grain,
            anchor: Anchor::Centre,
            choice: None,
            error: None,
        }));
    }

    fn open_batch_action(&mut self) {
        let selection: Vec<_> = self
            .selection
            .ids
            .iter()
            .copied()
            .filter(|id| self.editor.project().boards.iter().any(|b| b.id == *id))
            .map(BoardSelection::Board)
            .collect();
        if let Ok(summary) = self.editor.selected_boards(&selection) {
            self.modals.set_batch_dimension(Some(BatchDialog {
                focus_on_open: true,
                project_id: self.editor.project().id,
                revision: self.editor.project().revision,
                anchors: summary
                    .board_ids
                    .iter()
                    .map(|id| (*id, Anchor::Centre))
                    .collect(),
                ids: summary.board_ids,
                dimension: BoardDimension::Length,
                value: DimensionDraft::new(),
                error: None,
            }));
        }
    }

    fn update_catalog_action(&mut self, id: Uuid) {
        self.hardware.catalog_update_notice = Some(
            match hardware_catalog::update_from_catalog_with_status(
                &mut self.editor,
                &self.hardware.catalogs,
                id,
                catalog_ui::language_tag(&self.localizer),
            ) {
                Ok((_, statuses)) => {
                    let details = statuses
                        .iter()
                        .map(|status| {
                            let warnings = if status.issues.is_empty() {
                                self.localizer.text("catalog-verified")
                            } else {
                                status
                                    .issues
                                    .iter()
                                    .map(|issue| self.localizer.text(hinge_ui::issue_key(issue)))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            };
                            format!("{}: {warnings}", status.id)
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    format!(
                        "{}: {} / {}. {details}",
                        self.localizer.text("hinge-update-reviewed"),
                        statuses.iter().filter(|s| s.issues.is_empty()).count(),
                        statuses.len()
                    )
                }
                Err(_) => self.localizer.text("hinge-update-error"),
            },
        );
    }
}

#[cfg(test)]
mod tests;
