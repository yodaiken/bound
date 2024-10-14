# bound

## Adjusted Metrics

The goal for the adjusted metrics is to weight changes within a commit to what
teams are actually getting contributed to.

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
  owner1 changes: 100
  owner2 changes: 50 + 25 = 75

  total changes: 100 + 75 = 175

  owner1 commits: owner1 changes / total changes = 100 / 175 = 0.5714
  owner2 commits: owner2 changes / total changes = 75 / 175 = 0.4286

commit 2:
  owner1 changes: 5
  owner2 changes: 25 + 10 = 35

  total changes: 5 + 35 = 40

  owner1 commits: owner1 changes / total changes = 5 / 40 = 0.125
  owner2 commits: owner2 changes / total changes = 35 / 40 = 0.875
```

Note that in the adjusted metrics, the number of commits will add to 1 for each
commit (which is likely intuitive for most analyses).
