//! Behavioural tests for container creation security profiles.

mod bdd_container_creation_helpers;

pub use bdd_container_creation_helpers::{ContainerCreationState, container_creation_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Create container in privileged mode"
)]
fn create_container_in_privileged_mode(container_creation_state: ContainerCreationState) {
    let _ = container_creation_state;
}

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Create container in minimal mode with /dev/fuse"
)]
fn create_container_in_minimal_mode_with_fuse(container_creation_state: ContainerCreationState) {
    let _ = container_creation_state;
}

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Create container in minimal mode without /dev/fuse"
)]
fn create_container_in_minimal_mode_without_fuse(container_creation_state: ContainerCreationState) {
    let _ = container_creation_state;
}

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Create container fails when image is missing"
)]
fn create_container_fails_when_image_missing(container_creation_state: ContainerCreationState) {
    let _ = container_creation_state;
}

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Create container fails when image is whitespace only"
)]
fn create_container_fails_when_image_whitespace_only(
    container_creation_state: ContainerCreationState,
) {
    let _ = container_creation_state;
}

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Create container surfaces engine create failures"
)]
fn create_container_surfaces_engine_failures(container_creation_state: ContainerCreationState) {
    let _ = container_creation_state;
}

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Privileged mode ignores /dev/fuse setting"
)]
fn privileged_mode_ignores_fuse_setting(container_creation_state: ContainerCreationState) {
    let _ = container_creation_state;
}

#[scenario(
    path = "tests/features/container_creation.feature",
    name = "Privileged mode ignores SELinux override"
)]
fn privileged_mode_ignores_selinux_override(container_creation_state: ContainerCreationState) {
    let _ = container_creation_state;
}
