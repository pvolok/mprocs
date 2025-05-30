use compact_str::CompactString;

pub trait TermReplySender {
  fn reply(&self, s: CompactString);
}
