// Deterministic 100-board/10-stock workload, in manufacturing micrometres.
// Ten identical 100 x 50 mm parts fit each 1050 x 60 mm owned stock piece
// with a 5 mm kerf (nine separators plus an edge isolation cut).
use plan_my_cabinet::domain::{
    Board, BoardGrain, Material, Project, Stock, StockGrain, StockSource,
};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{Length, Pose, Quaternion};
use uuid::Uuid;

pub fn fixture() -> Project {
    let mm = |n: i64| Length::from_micrometres(n * 1000);
    let id = Uuid::from_u128;
    let mut project = Project::new("100-board performance", Currency::Brl);
    project.id = id(1);
    project.cutting_kerf = mm(5);
    project.materials.push(Material {
        id: id(2),
        name: "Plywood".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Length,
    });
    for i in 0..100 {
        project.boards.push(Board {
            id: id(100 + i),
            name: format!("Part {:03}", i + 1),
            material_id: id(2),
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
    }
    for i in 0..10 {
        project.stock.push(Stock {
            id: id(1000 + i),
            name: format!("Sheet {:02}", i + 1),
            material_id: id(2),
            length: mm(1050),
            width: mm(60),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::Owned,
            price: None,
            priority: i as u32,
            trim: [Length::ZERO; 4],
        });
    }
    project.validate().unwrap();
    project
}
