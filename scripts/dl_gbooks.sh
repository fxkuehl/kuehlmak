#!/bin/sh

# https://storage.googleapis.com/books/ngrams/books/datasetsv2.html
# https://aclanthology.org/P12-3029.pdf

if [ -z "$1" ]; then
	echo "Usage: $0 <language> [-2]"
	echo
	echo "Please specify the language to download:"
	echo "  eng, eng-us, eng-gb, eng-fiction, chi-sim, fre, ger, heb, ita, rus, spa"
	exit 1
fi

lang=$1
if [ "$2" = "-2" ]; then
	bigrams="y"
else
	bigrams=""
fi

NUMBERS="0 1 2 3 4 5 6 7 8 9"
LETTERS="a b c d e f g h i j k l m n o p q r s t u v w x y z"

touch -d20130101 marker.txt

for x in $NUMBERS $LETTERS other punctuation; do
	if [ -f googlebooks-$lang-all-1gram-20120701-$x.gz -a \
		googlebooks-$lang-all-1gram-20120701-$x.gz -ot marker.txt ]; then
		echo "Skipping googlebooks-$lang-all-1gram-20120701-$x.gz."
		continue
	fi
	wget -q --show-progress --continue http://storage.googleapis.com/books/ngrams/books/googlebooks-$lang-all-1gram-20120701-$x.gz
done

if [ -z "$bigrams" ]; then
	echo "Skipping bigrams. Use the option '-2' to enable bigram download."
	exit 0
fi

for x in $NUMBERS other punctuation; do
	if [ -f googlebooks-$lang-all-2gram-20120701-$x.gz -a \
		googlebooks-$lang-all-2gram-20120701-$x.gz -ot marker.txt ]; then
		echo "Skipping googlebooks-$lang-all-2gram-20120701-$x.gz."
		continue
	fi
	wget -q --show-progress --continue http://storage.googleapis.com/books/ngrams/books/googlebooks-$lang-all-2gram-20120701-$x.gz
done
for x in $LETTERS; do
	for y in _ $LETTERS; do
		if [ -f googlebooks-$lang-all-2gram-20120701-$x$y.gz -a \
			googlebooks-$lang-all-2gram-20120701-$x$y.gz -ot marker.txt ]; then
			echo "Skipping googlebooks-$lang-all-2gram-20120701-$x$y.gz."
			continue
		fi
		wget -q --show-progress --continue http://storage.googleapis.com/books/ngrams/books/googlebooks-$lang-all-2gram-20120701-$x$y.gz
	done
done
