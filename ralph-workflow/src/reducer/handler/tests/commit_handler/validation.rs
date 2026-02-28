use super::super::common::TestFixture;
use crate::reducer::event::{CommitEvent, PipelineEvent};
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::reducer::ui_event::{UIEvent, XmlOutputType};

#[test]
fn validate_commit_xml_emits_ui_xml_output_even_when_xml_file_missing() {
    let mut fixture = TestFixture::new();
    let ctx = fixture.ctx();

    let handler = MainEffectHandler::new(PipelineState::initial(1, 0));

    let result = handler.validate_commit_xml(&ctx);

    assert!(
        matches!(
            result.event,
            PipelineEvent::Commit(CommitEvent::CommitXmlValidationFailed { attempt: 1, .. })
        ),
        "expected CommitXmlValidationFailed event when xml is missing, got: {:?}",
        result.event
    );

    assert!(
        result.ui_events.iter().any(|e| matches!(
            e,
            UIEvent::XmlOutput {
                xml_type: XmlOutputType::CommitMessage,
                ..
            }
        )),
        "expected UIEvent::XmlOutput(CommitMessage) even when xml missing"
    );
}
