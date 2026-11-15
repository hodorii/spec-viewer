# 구현 계획 - tasks-view-sample

## 정의
작업 목록 구조화 뷰를 시험하기 위한 샘플 작업 목록이다.

- [x] 1. 가입 기반
- [x] 1.1 (P) 가입 양식 검증
  - 이메일과 비밀번호 규칙을 검사한다
  - _DoneWhen: 잘못된 입력이 오류로 거부된다_
  - _Requirements: 1.1, 1.2_
  - _BizProcess: 가입 양식을 제출한다_
  - _Boundary: signup::form_
- [x] 1.2 (P) 계정 저장
  - DONE: 새 계정이 저장소에 기록된다
  - _Requirements: 1.1_
  - _Boundary: signup::store_
- [ ] 2. 로그인 기능
- [ ] 2.1 세션 발급
  - _DoneWhen: 올바른 자격이면 세션이 시작된다_
  - _Requirements: 2.1~2.3_
  - _BizProcess: 세션을 관리한다_
  - _Depends: 1.1, 1.2_
- [ ] 2.2 잠금 안내
  - _Requirements: 2.5_
  - _Depends: 2.1_
- [ ]* 3.1 탈퇴 확인 절차
  - 선택 작업이며 구형 형식 제목과 연결된다
  - _Requirements: 3.1_
  - _BizProcess: 탈퇴를 처리한다_
  - _Difficulty: low_
