use super::*;

#[test]
fn same_filesystem_move_preserves_the_checkpointed_directory_identity() {
    let root = temporary_root("rename");
    let plan = plan(&root);
    fs::create_dir(&plan.source_install_dir).unwrap();
    fs::write(plan.source_install_dir.join("payload"), b"value").unwrap();
    let source_identity = directory_identity(&plan.source_install_dir).unwrap();
    move_no_replace(&plan.source_install_dir, &plan.destination_install_dir).unwrap();

    assert!(!plan.source_install_dir.exists());
    let destination_identity = directory_identity(&plan.destination_install_dir).unwrap();
    require_directory_identity(
        "same-filesystem migration destination",
        &source_identity,
        &destination_identity,
    )
    .unwrap();
    assert_eq!(
        fs::read(plan.destination_install_dir.join("payload")).unwrap(),
        b"value"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn streaming_copy_preserves_bytes_and_symlink_objects() {
    let root = temporary_root("copy");
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(source.join("nested").join("payload"), b"payload").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("nested/payload", source.join("link")).unwrap();
    let cancellation = CancellationToken::default();
    copy_tree(&source, &destination, &cancellation).unwrap();
    assert_eq!(
        fs::read(destination.join("nested").join("payload")).unwrap(),
        b"payload"
    );
    #[cfg(unix)]
    assert_eq!(
        fs::read_link(destination.join("link")).unwrap(),
        PathBuf::from("nested/payload")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn instance_copy_uses_shared_readonly_git_file_transfer() {
    let root = temporary_root("readonly-git-pack");
    let source = root.join("source");
    let destination = root.join("destination");
    let relative = Path::new("core/.git/objects/pack/pack-test.idx");
    let source_pack = source.join(relative);
    fs::create_dir_all(source_pack.parent().unwrap()).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(&source_pack, b"git pack index").unwrap();
    let mut permissions = fs::metadata(&source_pack).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&source_pack, permissions).unwrap();
    let cancellation = CancellationToken::default();
    copy_tree(&source, &destination, &cancellation).unwrap();

    let destination_pack = destination.join(relative);
    assert_eq!(fs::read(&destination_pack).unwrap(), b"git pack index");
    assert!(
        fs::metadata(destination_pack)
            .unwrap()
            .permissions()
            .readonly()
    );
    crate::internal::hanafubuki_fs_transfer::remove_dir_all(&root).unwrap();
}

#[test]
fn cancelled_streaming_copy_stops_without_mutating_source() {
    let root = temporary_root("cancel");
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("payload"), b"payload").unwrap();
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let destination = root.join("destination");
    fs::create_dir(&destination).unwrap();
    assert!(
        copy_tree(&source, &destination, &cancellation)
            .unwrap_err()
            .contains("interrupted")
    );
    assert_eq!(fs::read(source.join("payload")).unwrap(), b"payload");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_removes_only_an_empty_staging_without_a_durable_identity() {
    let root = temporary_root("empty-staging-recovery");
    let plan = plan(&root);
    fs::create_dir(&plan.source_install_dir).unwrap();
    let journal = InstanceMaintenanceJournal::new(root.join("journal.json"));
    let entry = journal
        .reserve(InstanceMaintenanceIntent::Migration(plan.clone()))
        .unwrap();
    mutate_migration_checkpoint(&journal, &entry.operation_id, None, |checkpoint| {
        checkpoint.transfer_mode = InstanceMigrationTransferMode::CrossFilesystemCopy;
        Ok(())
    })
    .unwrap();
    let checkpoint =
        migration_checkpoint(&journal.get_entry(&entry.operation_id).unwrap()).unwrap();
    let staging = migration_staging_path(&plan, &entry.operation_id).unwrap();
    fs::create_dir(&staging).unwrap();

    super::super::transfer::restart_interrupted_copy(
        &journal,
        &entry.operation_id,
        &plan,
        &staging,
        &checkpoint,
    )
    .unwrap();

    assert!(!staging.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_never_deletes_a_replaced_staging_directory() {
    let root = temporary_root("replaced-staging-recovery");
    let plan = plan(&root);
    fs::create_dir(&plan.source_install_dir).unwrap();
    let journal = InstanceMaintenanceJournal::new(root.join("journal.json"));
    let entry = journal
        .reserve(InstanceMaintenanceIntent::Migration(plan.clone()))
        .unwrap();
    let staging = migration_staging_path(&plan, &entry.operation_id).unwrap();
    fs::create_dir(&staging).unwrap();
    let original_identity = directory_identity(&staging).unwrap();
    mutate_migration_checkpoint(&journal, &entry.operation_id, None, |checkpoint| {
        checkpoint.transfer_mode = InstanceMigrationTransferMode::CrossFilesystemCopy;
        checkpoint.transfer_root_identity = Some(original_identity.clone());
        Ok(())
    })
    .unwrap();
    fs::rename(&staging, root.join("replaced-staging-original")).unwrap();
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("user-file"), b"keep").unwrap();
    let checkpoint =
        migration_checkpoint(&journal.get_entry(&entry.operation_id).unwrap()).unwrap();

    let error = super::super::transfer::restart_interrupted_copy(
        &journal,
        &entry.operation_id,
        &plan,
        &staging,
        &checkpoint,
    )
    .unwrap_err();

    assert!(error.contains("identity changed"));
    assert_eq!(fs::read(staging.join("user-file")).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}
