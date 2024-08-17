#!/bin/sh

script=$(realpath $0)
kuehlmak=${script%/*}/../target/release/kuehlmak

input=$1
shift 1

wikiextractor -o - $input | egrep -v '^</?doc' | $kuehlmak corpus "$@"
