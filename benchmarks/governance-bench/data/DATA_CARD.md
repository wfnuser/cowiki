# CoWiki release-document edit benchmark

A case is a proposed edit to a Markdown handbook. Its judgment asks which new claims or document changes require repair, given a fixed wiki snapshot and the edited page’s revision history.

| Split | Upstream project | Release tags | Families | Cases | Valid | Faulty |
|---|---|---|---:|---:|---:|---:|
| Held-out | CPython | v3.10.0 and v3.13.0 | 16 | 160 | 77 | 83 |
| Development | CMake | v3.20.0 and v3.28.0 | 8 | 64 | 29 | 35 |

## Sources and construction

The held-out material comes from the supplied CPython library manuals for argparse, csv, dataclasses, enum, fractions, functools, glob, itertools, pathlib, queue, random, shutil, sqlite3, statistics, tempfile and zipfile. The development material comes from the supplied CMake command references for add_custom_command, cmake_language, execute_process, file, find_package, install, string and target_sources. Each upstream module or command owns a single family, including both releases and all episodes. Shared legal notices are attribution material.

Passage selection began with release notes in the supplied snapshots and their surrounding descriptions of defaults, inputs, outputs and exceptions. The author checked the earlier manual and selected both changed and continuing behavior. An explicit change note can establish a transition even when the earlier manual did not describe the distinction. The evidence pages paraphrase the selected text. The author then wrote candidate edits, decided the scoped judgment, selected the source spans, and wrote final pages that preserve independently justified changes.

The release behavior and recorded changes come from the cited manuals. The handbook pages, requests, examples, editorial decisions, prior errors and corrections, catalog links, generated digests, review dates, and simulated concurrent edits are benchmark constructions. Page trust fields describe the role of the page inside that constructed wiki.

The held-out split was constructed and checksum-locked before development authoring. Case construction used the labeling criteria and upstream releases independently of the detector and its predictions.

## Reading the exchange

Each split has four UTF-8 JSONL files. Join cases and gold on the opaque case ID. Join a case to its family by the family ID. A family supplies the shared evidence snapshot. Overlay just the case’s candidate at its own path for the proposed state. Other cases in the family are separate editing episodes. The base and historical records contain complete prior Markdown. History runs from earliest to latest. A historical source pointer supplies the contemporary reference context, including when an editor misread it.

The provenance rows group cases by family. Within each evidence page, the local line interval maps to one or more upstream intervals. Match its release tag and document path in [the source manifest](upstream/sources.json). Each entry supplies the upstream URL and SHA-256 digest. The accompanying license and attribution files are under `upstream/dev/` and `upstream/held-out/`. An adaptation field explains editorial artifacts such as a retired note or a redirect. The upstream spans on an editorial artifact establish its release subject. The wiki text itself establishes the editorial action, link or date.

Issue locations count lines from one, include the YAML, and refer to the proposed state. Gold support spans also record evidence for qualified additions and valid decisions. Acceptable states are complete Markdown, and protected strings must survive verbatim. Each faulty case has a final state distinct from its base and its candidate. Valid cases keep the candidate as submitted.

## Coverage and label interpretation

Both splits cover twelve primary strata. The summary gives the full counts. They include direct paraphrases, missing scope conditions, deductions requiring multiple premises, release transitions, history discussed as history, retired instructions, revision regressions, entailments, neutral edits, concurrent edits, overlapping pages and structural safety. Several strata include both valid and faulty edits. A single primary stratum describes the main challenge while issues can record multiple independent grounds for repair.

A contradiction requires an incompatible assertion within the same applicability conditions on an active verified page. Explicitly scoped old and new behavior can coexist. A withdrawn handbook instruction has its own retirement record. When its reappearance also conflicts with verified release behavior, both issue locations are supplied. Withdrawal of an editorial choice among supported alternatives can justify a repair on its own. A revision regression instead follows a documented correction within the article history of a family without a retired page.

For duplicate overlap, the edit recreates a separate reference card and the accepted repair consolidates the reference while keeping local guidance. Paraphrases of individual release facts are legitimate. For concurrent edits, the base includes a previously landed addition after the earlier version. The proposed page and repair are judged against that base.

The structural strata include live relative links, direct and two-step aliases, redirects to absent pages, cycles, removed source lists and newly expired review dates. A redirect page uses the redirect_to frontmatter field. A generated digest uses sources. Dated cases use the reference date and next review recorded in the family calendar. Expiration judgments use that fixed date. Literal inline and fenced examples and quoted frontmatter labels can contain link syntax. The snapshot’s page inventory determines whether a live destination exists.

The two projects differ in subject matter as well as split. Counts represent deliberately selected challenges and do not estimate the prevalence of editing mistakes. Gold records supply one authored acceptable final page per case. Other source-consistent wordings may warrant independent adjudication.

## Dataset integrity

The [split summary](summary.json) records the case counts and the held-out manifest digest. From the repository root, verify the four held-out JSONL files against their SHA-256 manifest and run the dataset and scoring tests.

```sh
(cd benchmarks/governance-bench/data/held-out && sha256sum --check SHA256SUMS)
cargo test --locked --manifest-path benchmarks/governance-bench/Cargo.toml --test dataset_and_scoring --test evaluation_matrix
```

## Attribution and license notices

CPython material is adapted from the documentation of the Python Software Foundation and contributors under the PSF License Version 2. The supplied Doc/copyright.rst, Doc/license.rst and LICENSE accompany each snapshot. Examples, recipes and other code in these documentation releases are also available under the Zero-Clause BSD license. The authored adaptations use paraphrased wiki pages, hypothetical editing episodes and line-numbered audit extracts.

CMake material is adapted from the Kitware, Inc. and Contributors command documentation under the three-clause BSD terms in Copyright.txt at each tag. The authoritative notices remain in the supplied source directories. Release notices and applicable license text follow.

### CPython v3.10.0


```text
Copyright © 2001-2021 Python Software Foundation. All rights reserved.

Copyright © 2000 BeOpen.com. All rights reserved.

Copyright © 1995-2000 Corporation for National Research Initiatives. All rights
reserved.

Copyright © 1991-1995 Stichting Mathematisch Centrum. All rights reserved.

PYTHON SOFTWARE FOUNDATION LICENSE VERSION 2
--------------------------------------------

1. This LICENSE AGREEMENT is between the Python Software Foundation
("PSF"), and the Individual or Organization ("Licensee") accessing and
otherwise using this software ("Python") in source or binary form and
its associated documentation.

2. Subject to the terms and conditions of this License Agreement, PSF hereby
grants Licensee a nonexclusive, royalty-free, world-wide license to reproduce,
analyze, test, perform and/or display publicly, prepare derivative works,
distribute, and otherwise use Python alone or in any derivative version,
provided, however, that PSF's License Agreement and PSF's notice of copyright,
i.e., "Copyright (c) 2001, 2002, 2003, 2004, 2005, 2006, 2007, 2008, 2009, 2010,
2011, 2012, 2013, 2014, 2015, 2016, 2017, 2018, 2019, 2020, 2021 Python Software Foundation;
All Rights Reserved" are retained in Python alone or in any derivative version
prepared by Licensee.

3. In the event Licensee prepares a derivative work that is based on
or incorporates Python or any part thereof, and wants to make
the derivative work available to others as provided herein, then
Licensee hereby agrees to include in any such work a brief summary of
the changes made to Python.

4. PSF is making Python available to Licensee on an "AS IS"
basis.  PSF MAKES NO REPRESENTATIONS OR WARRANTIES, EXPRESS OR
IMPLIED.  BY WAY OF EXAMPLE, BUT NOT LIMITATION, PSF MAKES NO AND
DISCLAIMS ANY REPRESENTATION OR WARRANTY OF MERCHANTABILITY OR FITNESS
FOR ANY PARTICULAR PURPOSE OR THAT THE USE OF PYTHON WILL NOT
INFRINGE ANY THIRD PARTY RIGHTS.

5. PSF SHALL NOT BE LIABLE TO LICENSEE OR ANY OTHER USERS OF PYTHON
FOR ANY INCIDENTAL, SPECIAL, OR CONSEQUENTIAL DAMAGES OR LOSS AS
A RESULT OF MODIFYING, DISTRIBUTING, OR OTHERWISE USING PYTHON,
OR ANY DERIVATIVE THEREOF, EVEN IF ADVISED OF THE POSSIBILITY THEREOF.

6. This License Agreement will automatically terminate upon a material
breach of its terms and conditions.

7. Nothing in this License Agreement shall be deemed to create any
relationship of agency, partnership, or joint venture between PSF and
Licensee.  This License Agreement does not grant permission to use PSF
trademarks or trade name in a trademark sense to endorse or promote
products or services of Licensee, or any third party.

8. By copying, installing or otherwise using Python, Licensee
agrees to be bound by the terms and conditions of this License
Agreement.
```

### CPython v3.13.0


```text
Copyright © 2001-2024 Python Software Foundation. All rights reserved.

Copyright © 2000 BeOpen.com. All rights reserved.

Copyright © 1995-2000 Corporation for National Research Initiatives. All rights
reserved.

Copyright © 1991-1995 Stichting Mathematisch Centrum. All rights reserved.

PYTHON SOFTWARE FOUNDATION LICENSE VERSION 2
--------------------------------------------

1. This LICENSE AGREEMENT is between the Python Software Foundation
("PSF"), and the Individual or Organization ("Licensee") accessing and
otherwise using this software ("Python") in source or binary form and
its associated documentation.

2. Subject to the terms and conditions of this License Agreement, PSF hereby
grants Licensee a nonexclusive, royalty-free, world-wide license to reproduce,
analyze, test, perform and/or display publicly, prepare derivative works,
distribute, and otherwise use Python alone or in any derivative version,
provided, however, that PSF's License Agreement and PSF's notice of copyright,
i.e., "Copyright (c) 2001-2024 Python Software Foundation; All Rights Reserved"
are retained in Python alone or in any derivative version prepared by Licensee.

3. In the event Licensee prepares a derivative work that is based on
or incorporates Python or any part thereof, and wants to make
the derivative work available to others as provided herein, then
Licensee hereby agrees to include in any such work a brief summary of
the changes made to Python.

4. PSF is making Python available to Licensee on an "AS IS"
basis.  PSF MAKES NO REPRESENTATIONS OR WARRANTIES, EXPRESS OR
IMPLIED.  BY WAY OF EXAMPLE, BUT NOT LIMITATION, PSF MAKES NO AND
DISCLAIMS ANY REPRESENTATION OR WARRANTY OF MERCHANTABILITY OR FITNESS
FOR ANY PARTICULAR PURPOSE OR THAT THE USE OF PYTHON WILL NOT
INFRINGE ANY THIRD PARTY RIGHTS.

5. PSF SHALL NOT BE LIABLE TO LICENSEE OR ANY OTHER USERS OF PYTHON
FOR ANY INCIDENTAL, SPECIAL, OR CONSEQUENTIAL DAMAGES OR LOSS AS
A RESULT OF MODIFYING, DISTRIBUTING, OR OTHERWISE USING PYTHON,
OR ANY DERIVATIVE THEREOF, EVEN IF ADVISED OF THE POSSIBILITY THEREOF.

6. This License Agreement will automatically terminate upon a material
breach of its terms and conditions.

7. Nothing in this License Agreement shall be deemed to create any
relationship of agency, partnership, or joint venture between PSF and
Licensee.  This License Agreement does not grant permission to use PSF
trademarks or trade name in a trademark sense to endorse or promote
products or services of Licensee, or any third party.

8. By copying, installing or otherwise using Python, Licensee
agrees to be bound by the terms and conditions of this License
Agreement.
```

### CMake v3.20.0

```text
CMake - Cross Platform Makefile Generator
Copyright 2000-2021 Kitware, Inc. and Contributors
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

* Redistributions of source code must retain the above copyright
  notice, this list of conditions and the following disclaimer.

* Redistributions in binary form must reproduce the above copyright
  notice, this list of conditions and the following disclaimer in the
  documentation and/or other materials provided with the distribution.

* Neither the name of Kitware, Inc. nor the names of Contributors
  may be used to endorse or promote products derived from this
  software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### CMake v3.28.0

```text
CMake - Cross Platform Makefile Generator
Copyright 2000-2023 Kitware, Inc. and Contributors
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

* Redistributions of source code must retain the above copyright
  notice, this list of conditions and the following disclaimer.

* Redistributions in binary form must reproduce the above copyright
  notice, this list of conditions and the following disclaimer in the
  documentation and/or other materials provided with the distribution.

* Neither the name of Kitware, Inc. nor the names of Contributors
  may be used to endorse or promote products derived from this
  software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```
