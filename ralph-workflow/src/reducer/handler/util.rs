use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::workspace::Workspace;
use std::path::Path;

pub(super) fn read_commit_message_xml(workspace: &dyn Workspace) -> Option<String> {
    read_xml_and_archive_if_present(workspace, Path::new(xml_paths::COMMIT_MESSAGE_XML))
}

/// Read XML content from the primary path only.
///
/// The reducer/effect pipeline requires agents to write XML to the canonical
/// `.agent/tmp/*.xml` paths. Archived `.processed` files are debug artifacts and
/// must not be used as fallback inputs.
pub(super) fn read_xml_if_present(
    workspace: &dyn Workspace,
    primary_path: &Path,
) -> Option<String> {
    workspace.read(primary_path).ok()
}

pub(super) fn read_xml_and_archive_if_present(
    workspace: &dyn Workspace,
    primary_path: &Path,
) -> Option<String> {
    let content = read_xml_if_present(workspace, primary_path);
    if content.is_some() {
        archive_xml_file_with_workspace(workspace, primary_path);
    }
    content
}
