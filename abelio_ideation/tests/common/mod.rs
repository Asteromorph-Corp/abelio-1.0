//! Shared fixtures for the agent integration tests.

#![allow(dead_code)]

use abelio_ideation::testing::*;

pub fn init() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
}

pub fn objective() -> Objective {
    let raw = include_str!("../fixtures/objective.json");
    serde_json::from_str(raw).expect("failed to parse fixture objective")
}

pub fn formulation() -> &'static str {
    "Design a cell-penetrating peptide that actively transcytoses the blood-brain barrier \
     and accumulates in brain parenchyma for CNS therapeutic delivery."
}

pub fn requirement() -> Requirement {
    Requirement {
        major: "Peptide must undergo receptor-mediated transcytosis across brain endothelium \
                while preserving cargo activity."
            .to_string(),
        minor: vec![
            "Must avoid lysosomal degradation after endocytosis.".to_string(),
            "Must remain non-immunogenic in repeated dosing.".to_string(),
            "Must release cargo in brain parenchyma, not endothelial cells.".to_string(),
        ]
        .into(),
    }
}

pub fn sample_elements() -> Elements {
    vec![
        "TAT peptide (HIV-1 transactivator)".to_string(),
        "Angiopep-2 (LRP1-targeting peptide)".to_string(),
        "Transferrin receptor binding motif".to_string(),
        "Glutathione transporter ligand".to_string(),
        "RVG peptide (rabies glycoprotein)".to_string(),
    ]
    .into()
}

pub fn sample_idea() -> &'static str {
    "A bispecific peptide chimera fusing Angiopep-2 for LRP1-mediated transcytosis with a \
     pH-sensitive endosomal-escape motif that releases cargo only in brain parenchyma."
}

pub fn sample_critique() -> &'static str {
    "The proposed LRP1 receptor saturation is not considered; at therapeutic doses LRP1 \
     transcytosis likely plateaus, bounding brain exposure well below the stated target."
}

pub fn sample_defense() -> &'static str {
    "The LRP1 saturation concern is mitigated by sub-stoichiometric dosing combined with \
     sustained-release formulation."
}
