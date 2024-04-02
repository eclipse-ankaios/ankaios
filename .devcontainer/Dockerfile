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

FROM ghcr.io/eclipse-ankaios/devcontainer-base:0.9.1

ARG USERNAME=vscode

# add git completion for bash
RUN curl -L https://raw.githubusercontent.com/git/git/master/contrib/completion/git-completion.bash -o /home/${USERNAME}/.git-completion.bash
RUN echo 'if [ -f ~/.git-completion.bash ]; then\n\
    . ~/.git-completion.bash\n\
fi\n' >> /home/$USERNAME/.bashrc

# add an alias for the test coverage generation
RUN echo 'alias cov="/workspaces/ankaios/tools/generate_test_coverage_report.sh"' | tee -a /home/${USERNAME}/.bashrc /home/${USERNAME}/.zshrc

USER ${USERNAME}

COPY .git_commit_template.txt /home/${USERNAME}/
