# Copyright (c) 2023 Elektrobit Automotive GmbH
#
# This program and the accompanying materials are made available under the
# terms of the Apache License, Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0.
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.
#
# SPDX-License-Identifier: Apache-2.0

FROM ghcr.io/eclipse-ankaios/devcontainer-base:0.9.2

ARG USERNAME=vscode

# add git completion for bash
RUN curl -L https://raw.githubusercontent.com/git/git/master/contrib/completion/git-completion.bash -o /home/${USERNAME}/.git-completion.bash
RUN echo 'if [ -f ~/.git-completion.bash ]; then\n\
    . ~/.git-completion.bash\n\
fi\n' >> /home/$USERNAME/.bashrc

# add an alias for the test coverage generation
RUN echo 'alias cov="/workspaces/ankaios/tools/generate_test_coverage_report.sh"' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc

# add aliases for development scripts and tools
RUN echo 'alias ankaios-start="/workspaces/ankaios/tools/dev_scripts/ankaios-start"' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc
RUN echo 'alias ankaios-clean="/workspaces/ankaios/tools/dev_scripts/ankaios-clean"' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc
RUN echo 'alias ank="/workspaces/ankaios/target/x86_64-unknown-linux-musl/debug/ank"' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc

# work in insecure mode in the dev container
RUN echo 'export ANKAGENT_INSECURE=true' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc
RUN echo 'export ANKSERVER_INSECURE=true' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc
RUN echo 'export ANK_INSECURE=true' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc

# add some convinence aliases
RUN echo 'alias ll="ls -la"' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc
RUN echo 'alias ..="cd .."' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc
RUN echo 'alias ...="cd ../.."' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc

USER ${USERNAME}

COPY .git_commit_template.txt /home/${USERNAME}/
