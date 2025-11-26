# Ankaios JSON schema automation

This crate's sole purpose is to generate the JSON schema used to validate Ankaios manifest.
The schema is generated automatically using the [schemars](https://github.com/GREsau/schemars) crate where the annotations to the objects are added in the [api](../api) crate itself.

## Generating a new schema

Building the crate results in updating the schema file `ank_schema.json` under `target`. If the rust-analyzer is running, you don't even need to manually build as the analyzer triggers the build automatically resulting in an update of the schema.

The [main.rs](./src/main.rs) still contain some code to output the schema to the console in case this is required.

For automation purposes and releasing the schema a separate, dedicated step would be required, but this is still to be defined.

## Using the schema

The schema file is already referenced in the [.vscode/settings.json](../.vscode/settings.json) and is associated with all files that have a `*.ank.y*ml` extension, e.g., "manifest.ank.yml".

Ultimately we shall publish the schema to a public schema store, e.g., <https://www.schemastore.org/> and associate `*.ank.yml` file with it s.t. manifest creation works out of the box even outside of the Ankaios development environment.
