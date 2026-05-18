# Test coverage

To generate the test coverage report, run the following commands in `ankaios` workspace which is `/home/vscode/workspaces/ankaios/`:

To print out directly into the console:

```
cov test
```

Or to produce a report in html:

```
cov test --html
```

The script outputs where to find the report html:

```
...
Finished report saved to /workspaces/ankaios/target/llvm-cov/html
```

Note: By the first usage you might be asked for confirmation to install the `llvm-tools-preview` tool.

While writing tests, you may want to execute only the tests in a certain file and check the reached coverage. To do so you can execute:

To print out directly into the console:

```
cov test ankaios_server
```

Or to produce a report in html:

```
cov test ankaios_server --html
```

Once the run is complete, you can check the report to see which lines are not covered yet.
