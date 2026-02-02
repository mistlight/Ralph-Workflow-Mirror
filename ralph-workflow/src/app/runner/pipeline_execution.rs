// Pipeline execution and phase context creation.
//
// This module contains:
// - run_pipeline: Main pipeline execution via reducer event loop
// - RunWithHandlersParams: Parameters for test entry points
// - Phase context creation helpers
// - Pipeline preparation and finalization

// Include sub-modules
include!("pipeline_execution/discovery.rs");
include!("pipeline_execution/entry_points.rs");
include!("pipeline_execution/pipeline.rs");
include!("pipeline_execution/helpers.rs");
