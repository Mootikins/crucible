use super::parse;
use crate::cli::*;

/// A bare `cru models` still lists chat models. The subcommand group is
/// optional, so every script that calls it today keeps working.
#[test]
fn bare_models_keeps_its_old_shape() {
    let Commands::Models { format, command } = parse(&["cru", "models"]) else {
        panic!("Expected Models command");
    };
    assert!(format.is_none());
    assert!(command.is_none());

    let Commands::Models { format, command } = parse(&["cru", "models", "-f", "json"]) else {
        panic!("Expected Models command");
    };
    assert_eq!(format, Some(OutputFormat::Json));
    assert!(command.is_none());
}

/// The three embedding commands parse, and each carries its argument.
#[test]
fn embeddings_subcommands_parse() {
    let Commands::Models {
        command: Some(ModelsCommands::Embeddings { command, .. }),
        ..
    } = parse(&["cru", "models", "embeddings"])
    else {
        panic!("Expected Models Embeddings command");
    };
    assert!(command.is_none());

    let Commands::Models {
        command:
            Some(ModelsCommands::Embeddings {
                command: Some(EmbeddingsCommands::Download { name }),
                ..
            }),
        ..
    } = parse(&["cru", "models", "embeddings", "download", "arctic-embed-m"])
    else {
        panic!("Expected Models Embeddings Download command");
    };
    assert_eq!(name, "arctic-embed-m");

    let Commands::Models {
        command:
            Some(ModelsCommands::Embeddings {
                command: Some(EmbeddingsCommands::Use { name }),
                ..
            }),
        ..
    } = parse(&["cru", "models", "embeddings", "use", "bge-base-en-v1.5"])
    else {
        panic!("Expected Models Embeddings Use command");
    };
    assert_eq!(name, "bge-base-en-v1.5");
}
