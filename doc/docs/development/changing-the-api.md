# Changing the API

The `api` module contains the proto message definitions of the external interface of Ankaios - the Control Interface.
Changes to the definitions include but are not limited to adding or removing messages and bringing changes to the existing ones.

Because of the current schema of generation, all of the objects used with the definitions are generated automatically. This include:

- **The external objects**, generated from the proto messages by `prost`. The configuration for this step is in the `build.rs` file of the `api` crate.
- **The spec objects**, generated from the external ones by the derive macros. The spec definitions that configure this process are at `api/build/internal_structs.rs`.
- **The json schema**, generated from the spec objects, process configured in the `api/build/schema_annotations.rs` script.

Whenever a change occur in the `api`, the following steps must be done as well:

- Check the configuration of the external objects, for derives and additional annotations. The resulting object is used for filtering an must have the required format.
- Check the configuration of the spec objects, depending on the change in the external interface, there might be needed to add further annotations, or change existing ones.
- If the change affects the CompleteState, the spec object must match the desired manifest format.
- Check the schema configuration, changes to the internal objects will produce changes in the schema. The schema must be valid with the desired manifest structure.
