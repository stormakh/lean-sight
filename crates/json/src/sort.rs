//! A small stable sort for the whole program. `slice::sort_by` makes a full copy of std's sort code (several KB)
//! for each element type and closure; with about 40 sort calls that was 180 KB of code, and all of it is resident
//! memory. Here one non-generic merge sort orders indexes through a `dyn` compare, and a small generic part moves
//! the elements. Stable, O(n log n), no unsafe.
use std::cmp::Ordering;

/// Stable sort of `v` by `cmp` (same result as `v.sort_by(cmp)`).
pub fn sort_by<T>(v: &mut [T], mut cmp: impl FnMut(&T, &T) -> Ordering) {
    if v.len() < 2 {
        return;
    }
    let idx = {
        let r: &[T] = v;
        sort_indexes(r.len(), &mut |a, b| cmp(&r[a], &r[b]))
    };
    permute(v, idx);
}

/// Stable sort of `v` by a key (same result as `v.sort_by_key(key)`).
pub fn sort_by_key<T, K: Ord>(v: &mut [T], mut key: impl FnMut(&T) -> K) {
    sort_by(v, |a, b| key(a).cmp(&key(b)));
}

/// Stable sort of `v` in its natural order (same result as `v.sort()`).
pub fn sort<T: Ord>(v: &mut [T]) {
    sort_by(v, |a, b| a.cmp(b));
}

/// The indexes 0..n in stable sorted order: insertion sort on runs of 16, then bottom-up merges.
fn sort_indexes(n: usize, cmp: &mut dyn FnMut(usize, usize) -> Ordering) -> Vec<usize> {
    let mut a: Vec<usize> = (0..n).collect();
    const RUN: usize = 16;
    let mut start = 0;
    while start < n {
        let end = (start + RUN).min(n);
        for i in start + 1..end {
            let mut j = i;
            while j > start && cmp(a[j - 1], a[j]) == Ordering::Greater {
                a.swap(j - 1, j);
                j -= 1;
            }
        }
        start = end;
    }
    if n <= RUN {
        return a;
    }
    let mut b = vec![0usize; n];
    let mut width = RUN;
    while width < n {
        let mut lo = 0;
        while lo < n {
            let mid = (lo + width).min(n);
            let hi = (lo + 2 * width).min(n);
            let (mut i, mut j, mut k) = (lo, mid, lo);
            while i < mid && j < hi {
                // take from the right run only when it is strictly smaller: stable
                if cmp(a[j], a[i]) == Ordering::Less {
                    b[k] = a[j];
                    j += 1;
                } else {
                    b[k] = a[i];
                    i += 1;
                }
                k += 1;
            }
            while i < mid {
                b[k] = a[i];
                i += 1;
                k += 1;
            }
            while j < hi {
                b[k] = a[j];
                j += 1;
                k += 1;
            }
            lo = hi;
        }
        std::mem::swap(&mut a, &mut b);
        width *= 2;
    }
    a
}

/// Reorders `v` so that the new `v[i]` is the old `v[idx[i]]`, by following the cycles of the permutation.
fn permute<T>(v: &mut [T], mut idx: Vec<usize>) {
    for i in 0..v.len() {
        let mut cur = i;
        loop {
            let next = idx[cur];
            idx[cur] = cur;
            if next == i || next == cur {
                break;
            }
            v.swap(cur, next);
            cur = next;
        }
    }
}
