# Requirements tracing

The Eclipse Ankaios project provides requirements tracing using the [Open Fast Trace](https://github.com/itsallcode/openfasttrace) requirement tracing suite. The dev container already includes an `oft` executable and supports the `oft` command. For convenience, you can run the `tools/generate_oft_html_report.sh` script to automatically generate an HTML report showing the current coverage state. The script automatically includes all src and doc folders in the root folder. As long as you name your folders accordingly they will be processed by `oft`. 

For details on the `oft` tool, please consult the [user documentation](https://github.com/itsallcode/openfasttrace/blob/main/doc/user_guide.md) or execute `oft help`.
