# Update state cases

Cases must be tested with `ank set state` and `ank apply`.

| User action                                            | Expected Ankaios action                              | covered by |
|--------------------------------------------------------|------------------------------------------------------|------------|
| Update config item                                     | Affected workloads shall be updated                  ||
| RuntimeConfig changes                                  | Affected workloads shall be updated                  ||
| Remove unused config item                              | No workloads shall be updated, but configs           ||
| Remove used config item or all config items            | ConfigRenderError, state not updated at all          ||
| Remove workload referencing config item                | Workload shall be removed, but configs kept          ||
| Remove config reference but keep another one           | ConfigRenderError, state not updated at all          ||
| Remove all config reference, and keep templated config | ConfigRenderError, state not updated at all          ||
