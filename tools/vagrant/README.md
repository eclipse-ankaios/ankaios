# Vagrant VMs

This folder contains different [Vagrant](https://developer.hashicorp.com/vagrant)
setups for virtual machines which are intended for testing Ankaios. And thus they
already come with Podman pre-installed.

## Setup

In order to use these virtual machines, Vagrant needs to be installed and also
VirtualBox, which is used by Vagrant to create the VMs. Vagrant should be
installed on the same host OS as VirtualBox, eg. on Windows.

For Vagrant installation see <https://developer.hashicorp.com/vagrant/install>.

## Creating and running a VM

Make sure to have this repo cloned on the host and then enter one of the
sub-folders (eg. `ubuntu22.04`) with the desired setup. 

Create the VM using:

```
vagrant up
```

When called for the first time, this might take some time as the base box (i.e. base VM) needs to be downloaded.

Login with the following command and `vagrant` as password if required:

```
vagrant ssh
```

Now you are in the VM and can execute any command like installing Ankaios and starting it.

## Destroy VM

To destroy the VM call:

```
vagrant destroy
```

## FAQ

**Question:** Where do I find other boxes (i.e. base images)?

**Answer:** Goto <https://app.vagrantup.com/boxes/search> and there you will find existing Vagrant boxes to start from.

**Q:** How can I share files between host and VM?

**A:** The folder keeping the `Vagrantfile` on the host is available as `/vagrant` in the VM. Other
[folders can be synced as well](https://developer.hashicorp.com/vagrant/docs/synced-folders/basic_usage).

