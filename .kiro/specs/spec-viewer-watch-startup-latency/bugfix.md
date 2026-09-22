# Bugfix — spec-viewer-watch-startup-latency

## 정의
spec-viewer 사용자를 위해 대형 디렉터리를 열 때 체감되는 첫 화면 지연을 재현·수정하는 성능 버그픽스 스펙이다.

## 재현 절차
1. 환경: macOS, `spec-viewer` release 빌드(`m`), 대상 디렉터리가 `node_modules`/`target` 등 빌드 산출물을 포함해 전체 파일 수가 수만~십만 개인 실제 작업 디렉터리. 확인된 예:
   - 대형 프로젝트 A의 서브프로젝트 1(`node_modules` 46,204개 파일)
   - 대형 프로젝트 A의 서브프로젝트 2(`node_modules` 32,565개 파일)
   - `~/dev/archgenworks/spec-viewer` (`target/` 147,286개 파일, 5.3GB)
2. 입력: `m --all <해당 디렉터리>` 실행 (또는 `--all` 없이도 감시 대상 루트 자체가 그 디렉터리인 경우 동일 증상).
3. 관찰 결과: `spec_viewer::watch::start(root, ..)`만 격리해 `std::time::Instant`로 측정 —
   - 대형 프로젝트 A 전체(5.3GB, 전체 서브프로젝트 포함): `FsTree::scan` 4~6ms(entries=18) 대비 `watch::start` 2.69s
   - 대형 프로젝트 A의 서브프로젝트 2: `FsTree::scan` 9ms(entries=1) 대비 `watch::start` 449ms
   - 대형 프로젝트 A의 서브프로젝트 1: `FsTree::scan` 9ms(entries=1) 대비 `watch::start` 297ms
   - `spec-viewer` 자신(`target/` 포함): `FsTree::scan` 2~3ms(entries=34) 대비 `watch::start` 1.5~3.37s(반복 측정에서도 재현)
   같은 디렉터리를 대상으로 한 기본(`.kiro`) 모드는 `.kiro` 루트만 감시하므로 (`.kiro` 자체가 968KB/60개 파일) `find_root`+`load_snapshot`+`spec::build` 합계 2~6ms로 영향받지 않음 — 증상은 감시 루트가 대형 디렉터리 자체가 되는 경로(`--all`, 또는 대형 디렉터리가 곧 `.kiro` 루트인 경우)에서만 나타남.
   `src/main.rs`의 `resolve_source`/`main` 흐름상 `watch::start`는 `run_loop`의 첫 `terminal.draw` 호출보다 먼저 실행되므로, 이 지연 동안 화면은 완전히 비어 있다.

## Boundary Context
- **In scope**: `watch::start`(및 이를 호출하는 `main()`의 시작 순서)가 대형 감시 루트에서 소비하는 초기화 시간, 그리고 그 시간이 화면 첫 렌더링을 블로킹하는 문제.
- **Out of scope**: `FsTree::scan`의 파일 목록 구성 로직(gitignore/hidden/깊이 6 필터는 정상 동작, 측정으로 확인됨), `.kiro` 로더(`load_snapshot`/`spec::build`) 경로(정상, 영향 없음), 감시 이벤트 디바운스·렌더링 로직 자체, `--no-watch` 플래그의 기존 동작.

## Behaviors

### 1. 현재 동작 (결함)
- 1.1: [`--all` 모드 또는 감시 대상 루트 자체가 `node_modules`/`target` 등으로 전체 파일 수가 수만 개 이상인 디렉터리] → 첫 화면(트리 패널)이 그려지기까지 수백 ms~3초 이상 걸리며, 그 동안 터미널이 완전히 빈 상태로 응답 없는 것처럼 보임.
- 1.2: [1.1과 동일 조건] → 지연 시간이 `.gitignore`/숨김/깊이 필터를 거쳐 트리에 실제로 표시되는 항목 수(1~34개)가 아니라, 감시 대상 루트 아래 전체 파일 수(필터 미적용)에 비례해 커짐.

### 2. 기대 동작
- 2.1: [1.1과 동일 조건] → 첫 화면(트리 패널)이 200ms 이내 그려짐 — 감시 초기화가 첫 렌더링을 블로킹하지 않음.
- 2.2: [1.2와 동일 조건] → 감시 초기화 비용이 감시 대상 루트 아래 전체 파일 수가 아니라, `FsTree::scan`이 이미 계산한 항목 수(같은 gitignore/hidden 필터 기준)에 비례함.

### 3. 불변 동작 (회귀 방지)
- 3.1: [파일 변경 발생, `--no-watch` 미지정, 항상] → 2초 이내 변경 이벤트가 여전히 감지되어 화면에 반영됨(요구사항 7.1 유지).
- 3.2: [`--no-watch` 지정, 항상] → 감시 초기화를 건너뛰고 상태 표시줄에 감시 꺼짐 표시, `r` 키로 수동 갱신 가능(요구사항 7.6 유지).
- 3.3: [`.kiro` 기반 기본 모드(비 `--all`), `.kiro` 루트 자체는 작은 경우, 항상] → 기존과 동일하게 수 ms 내 시작(이번 수정으로 인한 회귀 없음).
