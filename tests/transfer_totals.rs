//! R1 — transfers never count as income/expense (plan I3) but are reported separately.

use insight::manager::biz::{tally, Totals};
use insight::pb::service::entry::{Entry, EntryKind};

fn e(kind: EntryKind, amount: i64, cp: Option<&str>) -> Entry {
    Entry {
        kind: kind as i32,
        amount,
        counterparty_kind: cp.map(str::to_string),
        ..Default::default()
    }
}

#[test]
fn transfers_do_not_change_net() {
    let base = vec![
        e(EntryKind::Income, 1000, None),
        e(EntryKind::Expense, 300, None),
    ];
    let with = {
        let mut v = base.clone();
        v.push(e(EntryKind::TransferOut, 400, Some("entry")));
        v.push(e(EntryKind::TransferIn, 50, Some("entry")));
        v
    };
    let (a, b) = (tally(&base), tally(&with));
    assert_eq!(
        (a.income, a.expense, a.net()),
        (b.income, b.expense, b.net())
    );
    assert_eq!((b.transfer_in, b.transfer_out), (50, 400));
    assert_eq!(b.cash_flow(), 700 + 50 - 400);
}

#[test]
fn saved_to_funds_counts_only_fund_cash_outflows() {
    let t = tally(&[
        e(EntryKind::TransferOut, 100, Some("fund_cash")),
        e(EntryKind::TransferOut, 40, Some("entry")),
        e(EntryKind::TransferIn, 30, Some("fund_cash")),
    ]);
    assert_eq!(t.saved_to_funds, 100);
    assert_eq!(t.transfer_out, 140);
}

#[test]
fn empty_is_zero() {
    assert_eq!(tally(&[]), Totals::default());
}
