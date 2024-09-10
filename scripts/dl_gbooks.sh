#!/bin/sh

if [ -z "$1" ]; then
	echo "Please specify the language to download:"
	echo "  eng, eng-us, eng-gb, eng-fiction, chi-sim, fre, ger, heb, ita, rus, spa"
	exit 1
fi

lang=$1

touch -d20130101 marker.txt

for x in a b c d e f g h i j k l m n o p q r s t u v w x y z other punctuation; do
	if [ -f googlebooks-$lang-all-1gram-20120701-$x.gz -a \
		googlebooks-$lang-all-1gram-20120701-$x.gz -ot marker.txt ]; then
		echo "Skipping googlebooks-$lang-all-1gram-20120701-$x.gz."
		continue
	fi
	wget --continue http://storage.googleapis.com/books/ngrams/books/googlebooks-$lang-all-1gram-20120701-$x.gz
done

for x in other punctuation; do
	if [ -f googlebooks-$lang-all-2gram-20120701-$x.gz -a \
		googlebooks-$lang-all-2gram-20120701-$x.gz -ot marker.txt ]; then
		echo "Skipping googlebooks-$lang-all-2gram-20120701-$x.gz."
		continue
	fi
	wget --continue http://storage.googleapis.com/books/ngrams/books/googlebooks-$lang-all-2gram-20120701-$x.gz
done
for x in a b c d e f g h i j k l m n o p q r s t u v w x y z; do
	for y in _ a b c d e f g h i j k l m n o p q r s t u v w x y z; do
		if [ -f googlebooks-$lang-all-2gram-20120701-$x$y.gz -a \
			googlebooks-$lang-all-2gram-20120701-$x$y.gz -ot marker.txt ]; then
			echo "Skipping googlebooks-$lang-all-2gram-20120701-$x$y.gz."
			continue
		fi
		wget --continue http://storage.googleapis.com/books/ngrams/books/googlebooks-$lang-all-2gram-20120701-$x$y.gz
	done
done
