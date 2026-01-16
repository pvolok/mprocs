use crate::term_types::winsize::Winsize;

pub trait Process {
  fn on_exited(&mut self);

  async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
  async fn write(&mut self, buf: &[u8]) -> std::io::Result<usize>;
  async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;

  fn send_signal(&mut self, sig: i32) -> std::io::Result<()>;

  async fn kill(&mut self) -> std::io::Result<()>;

  fn resize(&mut self, size: Winsize) -> std::io::Result<()>;
}
