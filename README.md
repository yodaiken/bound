# bound

## Adjusted Metrics

The goal for the adjusted metrics is to weight changes within a commit to what
teams are actually getting contributed to, considering both insertions and deletions.

For example for these two commits:

```
commit 1:
  file1 owner1: +100, -50
  file2 owner2: +50, -25
  file3 owner2: +25, -10

commit 2:
  file1 owner1: +5, -10
  file2 owner2: +25, -10
  file3 owner2: +10, -5
```

Then the adjusted metrics would be:

```
commit 1:
  owner1 changes: 100 + 50 = 150
  owner2 changes: (50 + 25) + (25 + 10) = 110

  total changes: 150 + 110 = 260

  owner1 commits: owner1 changes / total changes = 150 / 260 = 0.5769
  owner2 commits: owner2 changes / total changes = 110 / 260 = 0.4231

commit 2:
  owner1 changes: 5 + 10 = 15
  owner2 changes: (25 + 10) + (10 + 5) = 50

  total changes: 15 + 50 = 65

  owner1 commits: owner1 changes / total changes = 15 / 65 = 0.2308
  owner2 commits: owner2 changes / total changes = 50 / 65 = 0.7692
```

Note that in the adjusted metrics, the number of commits will add to 1 for each
commit (which is likely intuitive for most analyses). The changes now include
both insertions and deletions, providing a more comprehensive view of the
total contributions for each owner.
