// Copyright (c) 2023 Elektrobit Automotive GmbH
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

pub trait UnreachableOption<T> {
    fn unwrap_or_unreachable(self) -> T;
}

impl<T> UnreachableOption<T> for Option<T> {
    /// Returns the contained [`Some`] value or panics
    /// by executing the unreachable! macro.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use common::std_extensions::extended_option::UnreachableOption;
    /// assert_eq!(Some::<&str>("foo").unwrap_or_unreachable(), "foo");
    ///
    /// // shall panic because unreachable is hit
    /// None::<&str>.unwrap_or_unreachable();
    /// ```
    fn unwrap_or_unreachable(self) -> T {
        match self {
            Some(value) => value,
            None => std::unreachable!(),
        }
    }
}
