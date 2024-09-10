#!/usr/bin/python3

import sys
import json

POS_TAGS = frozenset(["NOUN", "VERB", "ADJ", "ADV", "PRON", "DET", "ADP", "NUM", "CONJ", "PRT", ".", "X", "START", "END"])

DEL_SPACE_BEFORE_SINGLE = ".,:;?!"
DEL_SPACE_BEFORE_MULTI = "'"
DEL_SPACE_BETWEEN_SINGLE = '"'

def del_space_before(word):
    return (len(word) == 1 and word in DEL_SPACE_BEFORE_SINGLE) or \
            (word[0] in DEL_SPACE_BEFORE_MULTI)

def del_space_between(word):
    return len(word) == 1 and word in DEL_SPACE_BETWEEN_SINGLE

def add_ngram(d, g, n):
    try:
        d[g] += n
    except Exception:
        d[g] = n

def sub_ngram(d, g, n):
    d[g] -= n
    #if d[g] == 0:
    #    del d[g]
    #el
    if d[g] < 0:
        print("%d-gram '%s' underflowed to %d" % (len(g), g, d[g]), file=sys.stderr)

def process_word(word, occur):
    """ Process word monograms """
    #if del_space_before(word):
    #    t = "\0\0\0"
    #else:
    t = "\0\0 "
    for c in word + ' ':
        t = t[1:] + c
        add_ngram(symbols, c, occur)
        if t[1] != '\0':
            b = t[1:]
            add_ngram(bigrams, b, occur)
        if t[0] != '\0':
            add_ngram(trigrams, t, occur)

def process_bigram(word1, word2, occur):
    """ Process word bigrams with space between words """
    if del_space_before(word2):
        sub_ngram( symbols, ' ', occur)
        sub_ngram( bigrams, ' ' + word2[0], occur)
        sub_ngram( bigrams, word1[-1] + ' ', occur)
        add_ngram( bigrams, word1[-1] + word2[0], occur)
        add_ngram(trigrams, word1[-1] + word2[0:2].ljust(2), occur)
    elif occur > 1 and del_space_between(word2):
        sub_ngram( symbols, ' ', occur//2)
        sub_ngram( bigrams, ' ' + word2[0], occur//2)
        sub_ngram( bigrams, word1[-1] + ' ', occur//2)
        add_ngram( bigrams, word1[-1] + word2[0], occur//2)
        add_ngram(trigrams, word1[-1] + word2[0:2].ljust(2), occur//2)
        sub_ngram(trigrams, ' ' + word2[0:2].ljust(2), occur//2)
        add_ngram(trigrams, word1[-1] + ' ' + word2[0], (occur + 1)//2)
    elif occur > 1 and del_space_between(word1):
        sub_ngram( symbols, ' ', occur//2)
        sub_ngram( bigrams, ' ' + word2[0], occur//2)
        sub_ngram( bigrams, word1[-1] + ' ', occur//2)
        add_ngram( bigrams, word1[-1] + word2[0], occur//2)
        add_ngram(trigrams, word1[-2:].rjust(2) + word2[0], occur//2)
        sub_ngram(trigrams, word1[-2:].rjust(2) + ' ', occur//2)
        add_ngram(trigrams, word1[-1] + ' ' + word2[0], (occur + 1)//2)
    else:
        add_ngram(trigrams, word1[-1] + ' ' + word2[0], occur)

def approx_bigrams():
    """ Approximate word bigrams if no bigram data is available, assuming that
    the probability of a word does not depend on its predecessor
    """
    word_starts = [item for item in trigrams.items() if item[0][0] == ' ']# + \
#                  [item for item in bigrams.items()
#                                 if del_space_before(item[0].rstrip())]
    word_ends = [item for item in bigrams.items() if item[0][1] == ' ']
    total_words = sum([value for (key, value) in word_ends])
    carry = 0
    for e in word_ends:
        for s in word_starts:
            count = e[1] * s[1] // total_words
            #carry += e[1] * s[1] % total_words
            if carry > total_words:
                count += 1
                carry -= total_words
            if count > 0:
                process_bigram(e[0][0], s[0].strip(), count)

def is_pos_tag(words):
    for w in words:
        tag = w.rsplit('_', 2)
        if len(tag) > 1 and tag[1] in POS_TAGS:
            break
        elif len(tag) > 2 and tag[2] in POS_TAGS:
            break
    else:
        return False
    return True

def sorted_dict(d):
    items = list(d.items())
    idx = list(range(len(items)))
    idx = reversed(sorted(idx, key=lambda i: items[idx[i]][1]))
    sorted_d = {}
    for i in idx:
        sorted_d[items[i][0]] = items[i][1]
    return sorted_d

symbols = {}
bigrams = {}
trigrams = {}
y0 = 0
y1 = 3000
count = 0
bigrams_processed = False

if len(sys.argv) >= 2:
    y0 = int(sys.argv[1])
if len(sys.argv) >= 3:
    y1 = int(sys.argv[2])

for line in sys.stdin:
    fields = line.split('\t', 3)
    if fields[0] == ' ':
        continue

    year = int(fields[1])
    if year < y0 or year > y1:
        continue

    words = fields[0].split(' ', 2)
    if len(words) > 2:
        print("\nToo many words:", words, file=sys.stderr)
        continue
    if is_pos_tag(words):
        continue

    if count % 1000000 == 0:
        print("Processed %dM records: %s                  \r" % (count/1000000, fields[0]), file=sys.stderr, end='')
    count = count + 1

    occur = int(fields[2])
    if len(words) == 1:
        process_word(words[0].lower(), occur)
    else:
        process_bigram(words[0].lower(), words[1].lower(), occur)
        bigrams_processed = True

if not bigrams_processed:
    print("\nApproximating word bigrams ...", file=sys.stderr)
    approx_bigrams()

symbols = sorted_dict(symbols)
bigrams = sorted_dict(bigrams)
trigrams = sorted_dict(trigrams)

json.dump({"symbols": symbols, "bigrams": bigrams, "trigrams": trigrams}, sys.stdout, ensure_ascii=False, indent=2)
