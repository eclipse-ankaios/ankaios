
# Eclipse self-service

The Eclipse Foundation offers self-service of GitHub resources.
We are using this self-service to customize Github settings, for example to change branch protection rules or other important settings of the Ankaios project.
The current GitHub configuration is hosted as code inside a separate repository called [.eclipsefdn](https://github.com/eclipse-ankaios/.eclipsefdn).

The settings are in jsonnet format and can be modified by contributors.

A detailed overview of the self-service please have a look into the [self-service handbook](https://www.eclipse.org/projects/handbook/#resources-github-self-service).

## Process of changing the settings

If a configuration needs to be changed the process is the following:

1. Fork the [.eclipsefdn](https://github.com/eclipse-ankaios/.eclipsefdn) repository.
2. Do the configuration changes (Use the [Eclipse playground](https://eclipse-ankaios.github.io/.eclipsefdn/playground/) for trying out the available settings).
3. Open a PR pointing from your fork's branch to the [.eclipsefdn](https://github.com/eclipse-ankaios/.eclipsefdn) repository.
4. Make sure that a review is requested from: Ankaios project committer, eclipsefdn-releng, eclipsefdn-security.
5. After the changes were approved by the reviewers, a member of Eclipse Foundation IT staff will merge the PR and applies the new settings by using the [otterdog cli](https://otterdog.readthedocs.io/en/latest/).
