//! rmcp binding: each tool deserializes its input, runs the matching
//! `Workspace` method on a blocking thread, and returns the JSON result (plus
//! PNG images for pictures). Service errors become tool errors carrying
//! `{code, message, hint, details}` so the agent can correct itself.
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use plan_my_cabinet::service::banding::*;
use plan_my_cabinet::service::design::*;
use plan_my_cabinet::service::exports::*;
use plan_my_cabinet::service::fittings::*;
use plan_my_cabinet::service::hardware::*;
use plan_my_cabinet::service::project::*;
use plan_my_cabinet::service::stock::*;
use plan_my_cabinet::service::vision::*;
use plan_my_cabinet::service::{ServiceResult, Workspace};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, GetPromptRequestParams, GetPromptResponse, GetPromptResult,
        Implementation, ListPromptsResult, ListResourcesResult, PaginatedRequestParams, Prompt,
        PromptArgument, PromptMessage, ReadResourceRequestParams, ReadResourceResponse,
        ReadResourceResult, Resource, ResourceContents, Role, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Serialize;

pub const INSTRUCTIONS: &str = include_str!("instructions.md");
pub const GUIDE: &str = include_str!("guide.md");

#[derive(Clone)]
pub(crate) struct PmcServer {
    workspace: Arc<Mutex<Workspace>>,
    tool_router: ToolRouter<Self>,
}

pub(crate) async fn serve(workspace: Workspace) -> Result<(), Box<dyn std::error::Error>> {
    let server = PmcServer {
        workspace: Arc::new(Mutex::new(workspace)),
        tool_router: PmcServer::project_router()
            + PmcServer::design_router()
            + PmcServer::vision_router()
            + PmcServer::stock_router()
            + PmcServer::hardware_router()
            + PmcServer::fittings_router(),
    };
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn text(value: &impl Serialize) -> Result<String, McpError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(format!("cannot encode the result: {e}"), None))
}

impl PmcServer {
    async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Workspace) -> ServiceResult<T> + Send + 'static,
    ) -> Result<ServiceResult<T>, McpError> {
        let workspace = Arc::clone(&self.workspace);
        tokio::task::spawn_blocking(move || {
            let mut guard = workspace.lock().unwrap_or_else(PoisonError::into_inner);
            work(&mut guard)
        })
        .await
        .map_err(|e| McpError::internal_error(format!("the tool stopped unexpectedly: {e}"), None))
    }

    /// Run a tool whose result is JSON.
    async fn call<T: Serialize + Send + 'static>(
        &self,
        work: impl FnOnce(&mut Workspace) -> ServiceResult<T> + Send + 'static,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.run(work).await? {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(text(&value)?)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(text(&error)?)]),
        })
    }

    /// Run a tool that returns pictures: JSON first, then one image per PNG.
    async fn call_images(
        &self,
        work: impl FnOnce(&mut Workspace) -> ServiceResult<Rendered> + Send + 'static,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.run(work).await? {
            Ok(rendered) => {
                let mut content = vec![ContentBlock::text(text(&rendered.json)?)];
                for png in &rendered.images {
                    content.push(ContentBlock::image(
                        base64::engine::general_purpose::STANDARD.encode(png),
                        "image/png",
                    ));
                }
                CallToolResult::success(content)
            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(text(&error)?)]),
        })
    }
}

// ---------------------------------------------------------------- project

#[tool_router(router = project_router)]
impl PmcServer {
    #[tool(
        description = "Read the domain guide: coordinates, board axes, units, references, the recommended workflow and worked examples. Call this first if you have not read the server instructions."
    )]
    async fn get_guide(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(GUIDE)]))
    }

    #[tool(
        description = "Start a new, empty project in memory (replaces the open one). Seeds the standard sheet materials by default. Nothing is written to disk until save_project."
    )]
    async fn new_project(
        &self,
        Parameters(input): Parameters<NewProjectInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.new_project(input)).await
    }

    #[tool(description = "Open a .pmcab project file (absolute path).")]
    async fn open_project(
        &self,
        Parameters(input): Parameters<OpenProjectInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.open_project(input)).await
    }

    #[tool(
        description = "Save the project. The first save needs an absolute path ending in .pmcab; later saves reuse it. Refuses to replace a different existing file unless overwrite is true (ask the user first)."
    )]
    async fn save_project(
        &self,
        Parameters(input): Parameters<SaveProjectInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.save_project(input)).await
    }

    #[tool(description = "Close the open project.")]
    async fn close_project(
        &self,
        Parameters(input): Parameters<CloseProjectInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.close_project(input)).await
    }

    #[tool(
        description = "Summary of the open project: path, revision, unsaved changes, units, kerf, cut fee, object counts, cut-plan status and cost estimate."
    )]
    async fn get_project(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.get_project()).await
    }

    #[tool(
        description = "Change project settings: name, display unit, grid spacing, saw kerf, shop kerf confirmation, cut fee (null = unknown)."
    )]
    async fn set_project_settings(
        &self,
        Parameters(input): Parameters<ProjectSettingsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.set_project_settings(input)).await
    }

    #[tool(
        description = "Change the project currency, either relabelling the amounts or replacing every price."
    )]
    async fn change_currency(
        &self,
        Parameters(input): Parameters<ChangeCurrencyInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.change_currency(input)).await
    }

    #[tool(
        description = "Undo the last change(s). Each tool call that changed something is usually one step (see undo_steps in its result)."
    )]
    async fn undo(
        &self,
        Parameters(input): Parameters<StepsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.undo(input)).await
    }

    #[tool(description = "Redo undone change(s).")]
    async fn redo(
        &self,
        Parameters(input): Parameters<StepsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.redo(input)).await
    }
}

// ----------------------------------------------------------------- design

#[tool_router(router = design_router)]
impl PmcServer {
    #[tool(
        description = "List materials with thickness, grain, color, usage and the standard sheet size when known."
    )]
    async fn list_materials(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.list_materials()).await
    }

    #[tool(
        description = "Create a material (name, default thickness, grain rule, optional color). Boards made of it take its thickness."
    )]
    async fn create_material(
        &self,
        Parameters(input): Parameters<CreateMaterialInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_material(input)).await
    }

    #[tool(
        description = "Add the standard sheet materials (MDF/HDF with known sheet sizes) to a project that has none."
    )]
    async fn seed_standard_materials(
        &self,
        Parameters(input): Parameters<SeedMaterialsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.seed_standard_materials(input)).await
    }

    #[tool(
        description = "Edit a material's name, thickness, grain or color, choosing whether existing boards follow (dependants). Use dry_run to see affected boards."
    )]
    async fn update_material(
        &self,
        Parameters(input): Parameters<UpdateMaterialInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.update_material(input)).await
    }

    #[tool(description = "Delete a material that no board or stock piece uses.")]
    async fn delete_material(
        &self,
        Parameters(input): Parameters<RefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.delete_material(input)).await
    }

    #[tool(
        description = "List boards with size, material, grain, world position and cut-plan status (sheet, origin, lock). Filter by assembly, material or status."
    )]
    async fn list_boards(
        &self,
        Parameters(input): Parameters<ListBoardsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.list_boards(input)).await
    }

    #[tool(
        description = "Everything about one board: size, pose (local and world), world bounds, grain, cut-plan status with reasons, hinges."
    )]
    async fn get_board(
        &self,
        Parameters(input): Parameters<GetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.get_board(input)).await
    }

    #[tool(
        description = "Create a board: length (local X) × width (local Y), thickness from the material (local Z). Position/rotation via pose or preset (stand_up = side panel, rx 90 = front/back panel). Auto-placed on declared stock when possible."
    )]
    async fn create_board(
        &self,
        Parameters(input): Parameters<CreateBoardInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_board(input)).await
    }

    #[tool(
        description = "Create several boards in one call (same fields as create_board). Stops at the first invalid one."
    )]
    async fn create_boards(
        &self,
        Parameters(input): Parameters<CreateBoardsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_boards(input)).await
    }

    #[tool(
        description = "Copy a board one or more times, each offset from the previous (parent frame). Copies are new parts, auto-placed on stock."
    )]
    async fn duplicate_board(
        &self,
        Parameters(input): Parameters<DuplicateBoardInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.duplicate_board(input)).await
    }

    #[tool(
        description = "Change a board's length, width and/or thickness, keeping the anchor face fixed. Reports sheet placements that no longer fit."
    )]
    async fn resize_board(
        &self,
        Parameters(input): Parameters<ResizeBoardInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.resize_board(input)).await
    }

    #[tool(
        description = "Set one dimension on several boards (or all boards of assemblies) at once."
    )]
    async fn resize_boards(
        &self,
        Parameters(input): Parameters<ResizeBoardsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.resize_boards(input)).await
    }

    #[tool(description = "Give a board another material; it takes that material's thickness.")]
    async fn set_board_material(
        &self,
        Parameters(input): Parameters<SetBoardMaterialInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.set_board_material(input)).await
    }

    #[tool(description = "Override a board's grain direction (or follow the material again).")]
    async fn set_board_grain(
        &self,
        Parameters(input): Parameters<SetBoardGrainInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.set_board_grain(input)).await
    }

    #[tool(
        description = "Set edge banding on boards: a preset for all four edges (auto, none, front, all_four), or edges (length_1, length_2, width_1, width_2, front, all) with a value (auto, on, off). Only MDF and MDP boards take banding; others are skipped. One undo step."
    )]
    async fn set_board_banding(
        &self,
        Parameters(input): Parameters<SetBoardBandingInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.set_board_banding(input)).await
    }

    #[tool(
        description = "List the project's edge bands with size, color, the materials using each as default, and banded metres."
    )]
    async fn list_edge_bands(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.list_edge_bands()).await
    }

    #[tool(
        description = "Create an edge band (name as the shop lists it, thickness, height, color). Make it a material's default band with update_material."
    )]
    async fn create_edge_band(
        &self,
        Parameters(input): Parameters<CreateEdgeBandInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_edge_band(input)).await
    }

    #[tool(description = "Rename or resize an edge band, or change its color.")]
    async fn update_edge_band(
        &self,
        Parameters(input): Parameters<UpdateEdgeBandInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.update_edge_band(input)).await
    }

    #[tool(description = "Remove an edge band that no board or material uses.")]
    async fn remove_edge_band(
        &self,
        Parameters(input): Parameters<EdgeBandRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.remove_edge_band(input)).await
    }

    #[tool(
        description = "Preview a part list for a shop (format cortecloud-json): parts with quantity, cabinet, material, size, banding and hole count, what drilling is left out and why, and the file itself. Needs only a valid design, no cut plan."
    )]
    async fn get_part_list(
        &self,
        Parameters(input): Parameters<PartListInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.get_part_list(input)).await
    }

    #[tool(
        description = "Write the design as a file for a shop (format cortecloud-json: import in CorteCloud with Serviço Completo › Carregar arquivo Cortecloud). Refuses to replace a file unless overwrite is true. Records a receipt; not an undo step."
    )]
    async fn export_design(
        &self,
        Parameters(input): Parameters<ExportDesignInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.export_design(input)).await
    }

    #[tool(description = "Rename a board, assembly or hardware item.")]
    async fn rename_object(
        &self,
        Parameters(input): Parameters<RenameInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.rename_object(input)).await
    }

    #[tool(
        description = "Delete a board, an assembly with everything in it, or a hardware item, with their sheet placements, hinges and doors. Use dry_run to see what goes."
    )]
    async fn delete_object(
        &self,
        Parameters(input): Parameters<RefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.delete_object(input)).await
    }

    #[tool(
        description = "Set a board's position (mm) and/or rotation (degrees, intrinsic X→Y→Z) in its parent's or the world frame; omitted values stay. Optional preset afterwards."
    )]
    async fn set_board_pose(
        &self,
        Parameters(input): Parameters<SetBoardPoseInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.set_board_pose(input)).await
    }

    #[tool(
        description = "Move a board so one of its faces lies against a face of another board, with alignment, offset and gap. The most reliable way to assemble parts without computing coordinates."
    )]
    async fn place_board_on_face(
        &self,
        Parameters(input): Parameters<PlaceOnFaceInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.place_board_on_face(input)).await
    }

    #[tool(
        description = "Move and/or rotate objects rigidly in world space about a pivot (assemblies carry their contents)."
    )]
    async fn transform_objects(
        &self,
        Parameters(input): Parameters<TransformInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.transform_objects(input)).await
    }

    #[tool(
        description = "Bounding box (min, max, size in mm) of objects in the world or another object's axes."
    )]
    async fn measure(
        &self,
        Parameters(input): Parameters<MeasureInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.measure(input)).await
    }

    #[tool(description = "The object tree: assemblies with their boards and hardware.")]
    async fn get_outliner(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.get_outliner()).await
    }

    #[tool(description = "Find objects by part of their name, id prefix or stock alias.")]
    async fn find_objects(
        &self,
        Parameters(input): Parameters<FindInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.find_objects(input)).await
    }

    #[tool(description = "One assembly: pose, boards, sub-assemblies and bounds.")]
    async fn get_assembly(
        &self,
        Parameters(input): Parameters<GetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.get_assembly(input)).await
    }

    #[tool(description = "Group objects into a new assembly (they keep their world positions).")]
    async fn group_objects(
        &self,
        Parameters(input): Parameters<GroupInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.group_objects(input)).await
    }

    #[tool(description = "Remove an assembly, moving its contents up one level.")]
    async fn ungroup_assembly(
        &self,
        Parameters(input): Parameters<RefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.ungroup_assembly(input)).await
    }

    #[tool(
        description = "Move objects into another assembly (or the top level), keeping world positions."
    )]
    async fn reparent_objects(
        &self,
        Parameters(input): Parameters<ReparentInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.reparent_objects(input)).await
    }

    #[tool(description = "Copy an assembly with all its boards (new parts), offset in world mm.")]
    async fn duplicate_assembly(
        &self,
        Parameters(input): Parameters<DuplicateAssemblyInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.duplicate_assembly(input)).await
    }

    #[tool(
        description = "Create (or, with `hardware`, update) a dimensioned non-wood reference box such as a handle or appliance. Not cut from stock. For feet use add_foot (real shapes from the catalog)."
    )]
    async fn set_placeholder_hardware(
        &self,
        Parameters(input): Parameters<PlaceholderInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.placeholder(input)).await
    }

    #[tool(
        description = "The cabinet templates (base, wall, drawers), their dimensions, material roles and defaults."
    )]
    async fn list_templates(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| Ok(ws.list_templates())).await
    }

    #[tool(
        description = "Build a complete cabinet carcass from a template (base, wall or drawers) as a new project or inside the open one, with standard sheets added and every board placed. Doors are not included: add them as boards."
    )]
    async fn generate_template(
        &self,
        Parameters(input): Parameters<GenerateTemplateInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.generate_template(input)).await
    }
}

// ----------------------------------------------------------------- vision

#[tool_router(router = vision_router)]
impl PmcServer {
    #[tool(
        description = "Describe the built geometry in words and numbers: each board's size, orientation and position, which boards touch, which INTERSECT (a design error), small gaps and floating parts. Use after every structural change."
    )]
    async fn describe_scene(
        &self,
        Parameters(input): Parameters<DescribeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.describe_scene(input)).await
    }

    #[tool(
        description = "Render a picture of the 3D model from a named view (iso, front, top, …) or free yaw/pitch, optionally hiding objects, isolating some, or highlighting some. Numbers in the picture match the returned legend."
    )]
    async fn render_view(
        &self,
        Parameters(input): Parameters<RenderViewInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call_images(move |ws| ws.render_view(input)).await
    }

    #[tool(
        description = "Render up to 6 pictures in one call, e.g. iso + front + top, or the same view with and without the doors."
    )]
    async fn render_views(
        &self,
        Parameters(input): Parameters<RenderViewsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call_images(move |ws| ws.render_views(input)).await
    }

    #[tool(
        description = "Draw the cut plan of one stock sheet (stock = alias like S1) or of every used sheet: parts, numbered saw cuts, kerf and offcuts."
    )]
    async fn render_sheets(
        &self,
        Parameters(input): Parameters<RenderSheetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call_images(move |ws| ws.render_sheets(input)).await
    }

    #[tool(
        description = "Render the model with a door swung open to an angle, to check that it clears its neighbours."
    )]
    async fn render_door_opening(
        &self,
        Parameters(input): Parameters<DoorOpeningInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call_images(move |ws| ws.render_door_opening(input))
            .await
    }
}

// ------------------------------------------------------------------ stock

#[tool_router(router = stock_router)]
impl PmcServer {
    #[tool(
        description = "List stock pieces (sheets and offcuts) in fill order with alias, size, grain, source, price, parts on it, cut count and utilization."
    )]
    async fn list_stock(
        &self,
        Parameters(input): Parameters<ListStockInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.list_stock(input)).await
    }

    #[tool(description = "One stock piece with its parts and numbered cut sequence.")]
    async fn get_stock_piece(
        &self,
        Parameters(input): Parameters<StockRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.get_stock_piece(input)).await
    }

    #[tool(
        description = "Declare sheets or offcuts you have (owned) or will buy (to_purchase), with size, grain, trims and price; then place waiting boards on them."
    )]
    async fn create_stock(
        &self,
        Parameters(input): Parameters<CreateStockInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_stock(input)).await
    }

    #[tool(description = "Edit a stock piece. Placements are kept; conflicts are reported.")]
    async fn update_stock(
        &self,
        Parameters(input): Parameters<UpdateStockInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.update_stock(input)).await
    }

    #[tool(description = "Set prices for stock pieces, all pieces of a material, or \"all\".")]
    async fn set_stock_prices(
        &self,
        Parameters(input): Parameters<SetPricesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.set_stock_prices(input)).await
    }

    #[tool(description = "Copy a stock piece.")]
    async fn duplicate_stock(
        &self,
        Parameters(input): Parameters<DuplicateStockInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.duplicate_stock(input)).await
    }

    #[tool(description = "Delete a stock piece (optionally taking its parts off first).")]
    async fn delete_stock(
        &self,
        Parameters(input): Parameters<DeleteStockInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.delete_stock(input)).await
    }

    #[tool(
        description = "Move a stock piece in the fill order (rank 1 is used first, e.g. leftovers before new sheets)."
    )]
    async fn reorder_stock(
        &self,
        Parameters(input): Parameters<ReorderStockInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.reorder_stock(input)).await
    }

    #[tool(
        description = "What sheets the boards still waiting for stock need, per material and thickness, with why any cannot be placed."
    )]
    async fn suggest_sheets(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.suggest_sheets()).await
    }

    #[tool(
        description = "Add the suggested standard sheets (to purchase) for waiting boards and place them. Materials without a known sheet size need create_stock."
    )]
    async fn add_needed_sheets(
        &self,
        Parameters(input): Parameters<AddNeededSheetsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.add_needed_sheets(input)).await
    }

    #[tool(
        description = "Pack boards onto the declared stock: fill_gaps places waiting boards only; replan repacks everything not locked."
    )]
    async fn auto_place(
        &self,
        Parameters(input): Parameters<AutoPlaceInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.auto_place(input)).await
    }

    #[tool(
        description = "The whole cut plan: used sheets, parts, cuts, boards not ready, usage and the cost estimate."
    )]
    async fn get_cut_plan(
        &self,
        Parameters(input): Parameters<CutPlanInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.get_cut_plan(input)).await
    }

    #[tool(
        description = "Everything that stands between the project and a shop-ready cut list, with a to-do list."
    )]
    async fn get_diagnostics(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.get_diagnostics()).await
    }

    #[tool(
        description = "Manual cut-plan edit: place/move boards on sheets, take them off, lock/unlock — all or nothing, checked for overlaps and a valid guillotine cut sequence."
    )]
    async fn edit_sheet(
        &self,
        Parameters(input): Parameters<EditSheetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.edit_sheet(input)).await
    }

    #[tool(
        description = "Search for a better cut plan (lowest spending, fewest cuts or least unused area) for up to max_seconds; returns ranked candidates and a search_id. Locked placements are kept."
    )]
    async fn optimize_cut_plan(
        &self,
        Parameters(input): Parameters<OptimizeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.optimize_cut_plan(input)).await
    }

    #[tool(
        description = "Apply a candidate from optimize_cut_plan. Fails if the project changed since the search."
    )]
    async fn apply_optimization(
        &self,
        Parameters(input): Parameters<ApplyOptimizationInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.apply_optimization(input)).await
    }
}

// --------------------------------------------------------------- hardware

#[tool_router(router = hardware_router)]
impl PmcServer {
    #[tool(
        description = "Hinge catalog packs available to pin (bundled and user packs): families, variants, K/R tables."
    )]
    async fn list_hinge_catalog(
        &self,
        Parameters(input): Parameters<ListCatalogInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.list_hinge_catalog(input)).await
    }

    #[tool(description = "Hardware models pinned to this project: hinges, drawer slides and feet.")]
    async fn list_project_catalog(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.list_project_catalog()).await
    }

    #[tool(
        description = "Pin a hinge model to the project (default: the bundled reviewed full-overlay kit)."
    )]
    async fn add_catalog_hinge(
        &self,
        Parameters(input): Parameters<AddCatalogHingeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.add_catalog_hinge(input)).await
    }

    #[tool(
        description = "Plan hinges for a door without changing anything: the side it hangs from, hinge edge, recommended count and positions, default K/R."
    )]
    async fn suggest_hinges(
        &self,
        Parameters(input): Parameters<SuggestHingesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.suggest_hinges(input)).await
    }

    #[tool(
        description = "Add hinges to a door board, hung from the side next to it. Count, positions, K/R and orientation default sensibly; returns cup and plate positions and any issues."
    )]
    async fn add_hinges(
        &self,
        Parameters(input): Parameters<AddHingesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.add_hinges(input)).await
    }

    #[tool(description = "Move a hinge along the door edge or change its K/R/E.")]
    async fn update_hinge(
        &self,
        Parameters(input): Parameters<UpdateHingeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.update_hinge(input)).await
    }

    #[tool(description = "Re-space all hinges of a door evenly.")]
    async fn space_hinges_evenly(
        &self,
        Parameters(input): Parameters<DoorRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.space_hinges_evenly(input)).await
    }

    #[tool(
        description = "Remove a hinge (a door left without hinges loses its door relationship)."
    )]
    async fn remove_hinge(
        &self,
        Parameters(input): Parameters<HingeRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.remove_hinge(input)).await
    }

    #[tool(
        description = "Hinges with their door, mount, position, K/R, drilling references and issues."
    )]
    async fn list_hinges(
        &self,
        Parameters(input): Parameters<ListHingesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.list_hinges(input)).await
    }

    #[tool(
        description = "Make a hinged board (or door assembly) a door that swings on its hinges; records the axis and opening limit. Needs hinges first."
    )]
    async fn create_door(
        &self,
        Parameters(input): Parameters<CreateDoorInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_door(input)).await
    }

    #[tool(description = "Remove a door relationship (hinges stay).")]
    async fn remove_door(
        &self,
        Parameters(input): Parameters<DoorRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.remove_door(input)).await
    }

    #[tool(description = "Doors with mount, hinge count, review status and opening limit.")]
    async fn list_doors(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.list_doors()).await
    }
}

// ------------------------------------------------- slides and feet

#[tool_router(router = fittings_router)]
impl PmcServer {
    #[tool(
        description = "Hardware catalogs available to pin: hinges, drawer slides (every length with travel, clearance and holes) and feet (shape and size). kind filters; reload re-reads user packs."
    )]
    async fn list_hardware_catalog(
        &self,
        Parameters(input): Parameters<ListHardwareCatalogInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.list_hardware_catalog(input)).await
    }

    #[tool(
        description = "Pin one drawer slide length to the project (by variant code, or family + length). add_slides pins automatically; use this to choose ahead."
    )]
    async fn add_catalog_slide(
        &self,
        Parameters(input): Parameters<AddCatalogSlideInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.add_catalog_slide(input)).await
    }

    #[tool(description = "Pin a foot model from the catalog to the project.")]
    async fn add_catalog_foot(
        &self,
        Parameters(input): Parameters<AddCatalogFootInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.add_catalog_foot(input)).await
    }

    #[tool(
        description = "Create a new drawer slide model (height, side clearance with tolerance, lengths with travel and holes) and pin every length. save_to_catalog also writes it to the app's user catalog (user-models.toml) for future projects. Values are checked with the catalog rules."
    )]
    async fn create_slide_model(
        &self,
        Parameters(input): Parameters<CreateSlideModelInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_slide_model(input)).await
    }

    #[tool(
        description = "Create a new foot model and pin it. Shapes: tapered (plastic cone/pyramid: top, bottom, height), post (tube + top plate + optional glide: chrome feet, straight table legs), frame (closed tube frame: industrial legs; bottom_width < top_width = trapezoid, = tube_width = V). The picture follows these dimensions. save_to_catalog writes it to user-models.toml."
    )]
    async fn create_foot_model(
        &self,
        Parameters(input): Parameters<CreateFootModelInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.create_foot_model(input)).await
    }

    #[tool(
        description = "Plan slides for a drawer without changing anything: finds the box sides and the carcass sides beside them, measures the gaps and depths, and picks the longest length of the family that fits."
    )]
    async fn suggest_slides(
        &self,
        Parameters(input): Parameters<SuggestSlidesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.suggest_slides(input)).await
    }

    #[tool(
        description = "Install a pair of drawer slides on a drawer (its assembly or any board of it): the longest length that fits, centred on the box sides, 2 mm behind the carcass front. Checks the side gaps against the slide's clearance (e.g. 12.7 +0.5/-0 mm: make the box 25.4 mm narrower than the opening), depth and height; returns hole positions. Default family TT45 Slowmotion."
    )]
    async fn add_slides(
        &self,
        Parameters(input): Parameters<AddSlidesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.add_slides(input)).await
    }

    #[tool(
        description = "Change a drawer's slides: another length or entry, the height or the setback; refits after boards moved."
    )]
    async fn update_slide(
        &self,
        Parameters(input): Parameters<UpdateSlideInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.update_slide(input)).await
    }

    #[tool(description = "Remove a drawer's slides.")]
    async fn remove_slide(
        &self,
        Parameters(input): Parameters<SlideRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.remove_slide(input)).await
    }

    #[tool(
        description = "Drawer slides with their product, length, gaps, hole positions (mm from each board's front edge) and issues."
    )]
    async fn list_slides(
        &self,
        Parameters(input): Parameters<ListSlidesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.list_slides(input)).await
    }

    #[tool(
        description = "Place feet (catalog code like generic-post-square-100, a pinned entry, or a created model). anchor top_center puts the mounting face centre at the pose (e.g. under the cabinet bottom); positions places several at once. Feet are not cut; raise the furniture yourself (transform_objects) so they stand on the floor at z = 0."
    )]
    async fn add_foot(
        &self,
        Parameters(input): Parameters<AddFootInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.add_foot(input)).await
    }

    #[tool(description = "Change a foot's model, name, parent or position.")]
    async fn update_foot(
        &self,
        Parameters(input): Parameters<UpdateFootInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call(move |ws| ws.update_foot(input)).await
    }

    #[tool(description = "Feet in the project with model, position and counts per model.")]
    async fn list_feet(&self) -> Result<CallToolResult, McpError> {
        self.call(|ws| ws.list_feet()).await
    }

    #[tool(
        description = "Render the model with a drawer pulled out on its slides (fraction 0..1 of the travel, or a distance), to see the box and the slide members."
    )]
    async fn render_drawer_opening(
        &self,
        Parameters(input): Parameters<DrawerOpeningInput>,
    ) -> Result<CallToolResult, McpError> {
        self.call_images(move |ws| ws.render_drawer_opening(input))
            .await
    }
}

// ------------------------------------------------- resources and prompts

const RESOURCES: &[(&str, &str, &str)] = &[
    (
        "pmc://guide",
        "Guide",
        "Domain guide and worked examples (markdown).",
    ),
    ("pmc://project", "Project", "Summary of the open project."),
    (
        "pmc://project/outliner",
        "Outliner",
        "Assemblies, boards and hardware as a tree.",
    ),
    (
        "pmc://project/scene",
        "Scene description",
        "Geometry in words: positions, contacts, overlaps.",
    ),
    (
        "pmc://project/cut-plan",
        "Cut plan",
        "Sheets, parts, cuts and cost.",
    ),
    (
        "pmc://project/diagnostics",
        "Diagnostics",
        "What blocks a shop-ready cut list.",
    ),
];

const BUILD_CABINET: &str = "Build a {kind} cabinet {width} mm wide, {height} mm high and {depth} mm deep with {doors} door(s).\n\
1. generate_template (kind, name, dimensions).\n\
2. get_outliner and describe_scene to learn the carcass: the front is at Y = 0, the sides' outer faces at X = 0 and X = width.\n\
3. Doors (full overlay): boards of the front material with rx 90 at y 0 (thickness toward -Y). With a 2 mm top/bottom reveal and a 3 mm gap between doors: height = cabinet height - 4, width = (cabinet width - 3·(doors-1)) / doors, z = 2. Put them in the cabinet assembly.\n\
4. describe_scene: no overlaps; render_views iso + front (orthographic) and iso with the doors hidden.\n\
5. add_needed_sheets; get_diagnostics until boards_with_problems is empty.\n\
6. Ask for prices or set them; set the cut fee; confirm the shop kerf if the user agrees.\n\
7. optimize_cut_plan, then apply_optimization; render_sheets.\n\
8. For each door: add_hinges, then create_door; render_door_opening at 90°.\n\
9. get_diagnostics, then save_project{save_path}.";

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PmcServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            "plan-my-cabinet",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(
            RESOURCES
                .iter()
                .map(|(uri, name, description)| {
                    Resource::new(*uri, *name)
                        .with_description(*description)
                        .with_mime_type(if *uri == "pmc://guide" {
                            "text/markdown"
                        } else {
                            "application/json"
                        })
                })
                .collect(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri.clone();
        let body = match uri.as_str() {
            "pmc://guide" => Ok(GUIDE.to_owned()),
            "pmc://project" => self
                .run(|ws| {
                    ws.get_project()
                        .map(|v| serde_json::to_value(v).unwrap_or_default())
                })
                .await?
                .map(|v| v.to_string()),
            "pmc://project/outliner" => self
                .run(|ws| ws.get_outliner())
                .await?
                .map(|v| v.to_string()),
            "pmc://project/scene" => self
                .run(|ws| ws.describe_scene(Default::default()))
                .await?
                .map(|v| v.to_string()),
            "pmc://project/cut-plan" => self
                .run(|ws| ws.get_cut_plan(Default::default()))
                .await?
                .map(|v| v.to_string()),
            "pmc://project/diagnostics" => self
                .run(|ws| ws.get_diagnostics())
                .await?
                .map(|v| v.to_string()),
            _ => {
                return Err(McpError::invalid_params(
                    format!("unknown resource {uri}"),
                    None,
                ));
            }
        };
        let body = body.map_err(|e| McpError::invalid_request(e.message, None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(body, uri)]).into())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let arg =
            |name: &str, description: &str| PromptArgument::new(name).with_description(description);
        Ok(ListPromptsResult::with_all_items(vec![
            Prompt::new(
                "build_cabinet",
                Some(
                    "Step-by-step: a cabinet from a template with doors, cut plan, hinges, pictures and a saved file.",
                ),
                Some(vec![
                    arg("kind", "base, wall or drawers"),
                    arg("width", "mm"),
                    arg("height", "mm"),
                    arg("depth", "mm"),
                    arg("doors", "number of doors (0-2)"),
                    arg("save_path", "absolute .pmcab path"),
                ]),
            ),
            Prompt::new(
                "review_project",
                Some(
                    "Explain the open project to the user: parts, pictures, cut plan, cost and open issues.",
                ),
                None,
            ),
        ]))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let args = request.arguments.clone().unwrap_or_default();
        let get = |key: &str, default: &str| {
            args.get(key)
                .and_then(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .or_else(|| Some(v.to_string()))
                })
                .unwrap_or_else(|| default.to_owned())
        };
        let text = match request.name.as_str() {
            "build_cabinet" => BUILD_CABINET
                .replace("{kind}", &get("kind", "base"))
                .replace("{width}", &get("width", "600"))
                .replace("{height}", &get("height", "720"))
                .replace("{depth}", &get("depth", "560"))
                .replace("{doors}", &get("doors", "1"))
                .replace("{save_path}", &args.get("save_path").map(|p| format!(" to {p}")).unwrap_or_default()),
            "review_project" => "Review the open project for the user: get_project, get_outliner, describe_scene (call out any overlaps), render_views (iso, front orthographic, iso with doors hidden), get_cut_plan, render_sheets, get_diagnostics. Summarize the parts per material, the sheets to buy and the cost, and list what is still open.".to_owned(),
            other => return Err(McpError::invalid_params(format!("unknown prompt {other}"), None)),
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)]).into())
    }
}
