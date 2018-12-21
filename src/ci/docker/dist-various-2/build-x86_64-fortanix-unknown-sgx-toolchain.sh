#!/bin/bash
# Copyright 2018 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

set -eux

dir_name=
lib=
target="x86_64_fortanix_unknown_sgx"

install_prereq()
{
    apt-get update
    apt-get install -y --no-install-recommends \
            build-essential \
            ca-certificates \
            cmake \
            git
}

#TODO Remove
build_unwind_dummy()
{
    # Build unwind
    mkdir -p libunwind/build
    pushd libunwind/build
    mkdir lib
    touch lib/libunwind.a
    ret=`readlink -f lib/libunwind.a`
    popd
    lib=${ret}
}
build_unwind()
{
    #build_unwind_dummy
    #return
    # Clone LLVM and libunwind
    git clone https://github.com/llvm-mirror/llvm.git
    git clone -b release_50 https://github.com/fortanix/libunwind.git

    # Build unwind
    mkdir -p libunwind/build
    pushd libunwind/build
    cmake -DCMAKE_BUILD_TYPE="RELEASE" -DRUST_SGX=1 -G "Unix Makefiles" -DLLVM_PATH=../../llvm/ ../
    make unwind_static
    readlink -f lib/libunwind.a
    ret=`readlink -f lib/libunwind.a`
    popd
    lib=${ret}
}

make_wdir()
{
    tgt=$1
    dir_name="${tgt}_temp"
    # In case of an unclean run, git clone could fail for an already cloned repo.
    rm -rf ${dir_name}
    mkdir -p ${dir_name}
}

install()
{
    tgt=$1
    src=$2

    dst="/${tgt}/lib"
    mkdir -p ${dst}
    cp $src $dst
}

install_prereq

#sets dir_name
make_wdir "${target}"
pushd ${dir_name}


#sets lib
build_unwind

install ${target} ${lib}

#TODO Copy the artifact out.
#TODO remove the folder once done.
#Go back to where we came from
set +x
popd
rm -rf ${dir_name} 
