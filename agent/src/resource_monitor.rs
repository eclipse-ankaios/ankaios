// Copyright (c) 2025 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

use api::ank_base::{CpuUsageInternal, FreeMemoryInternal};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind};

#[cfg(test)]
use mockall::automock;

#[cfg(not(test))]
use sysinfo::System;

#[cfg(test)]
use tests::MockSystem as System;

pub struct ResourceMonitor {
    refresh_kind: RefreshKind,
    sys: System,
}

#[cfg_attr(test, automock)]
// [impl->swdd~agent-provides-resource-metrics~1]
impl ResourceMonitor {
    pub fn new() -> Self {
        let refresh_kind = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::nothing().with_ram());
        ResourceMonitor {
            refresh_kind,
            sys: System::new_with_specifics(refresh_kind),
        }
    }

    pub fn sample_resource_usage(&mut self) -> (CpuUsageInternal, FreeMemoryInternal) {
        self.sys.refresh_specifics(self.refresh_kind);

        let cpu_usage = self.sys.global_cpu_usage();
        let free_memory = self.sys.free_memory();

        (
            CpuUsageInternal::new(cpu_usage),
            FreeMemoryInternal { free_memory },
        )
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
// [utest->swdd~agent-provides-resource-metrics~1]
mod tests {

    pub struct MockSystem {
        refresh_kind: RefreshKind,
        cpu_usage: f32,
        free_memory: u64,
    }

    impl MockSystem {
        pub fn new_with_specifics(refresh_kind: RefreshKind) -> Self {
            MockSystem {
                refresh_kind,
                cpu_usage: 0.0,
                free_memory: 0,
            }
        }

        pub fn refresh_specifics(&mut self, refresh_kind: RefreshKind) {
            self.refresh_kind = refresh_kind;
            self.cpu_usage = 25.0;
            self.free_memory = 2048;
        }

        pub fn global_cpu_usage(&self) -> f32 {
            self.cpu_usage
        }

        pub fn free_memory(&self) -> u64 {
            self.free_memory
        }
    }

    use super::ResourceMonitor;
    use super::{CpuRefreshKind, MemoryRefreshKind, RefreshKind};

    #[test]
    fn utest_sample_resource_usage() {
        let mut resource_monitor = ResourceMonitor::new();
        let (cpu_usage, free_memory) = resource_monitor.sample_resource_usage();
        assert_eq!(cpu_usage.cpu_usage, 25);
        assert_eq!(free_memory.free_memory, 2048);
        assert_eq!(
            resource_monitor.refresh_kind,
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                .with_memory(MemoryRefreshKind::nothing().with_ram())
        );
    }
}
