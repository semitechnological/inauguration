defmodule Sample do
  def answer do
    42
  end

  def main do
    answer()
    :ok
  end

  defstruct value: 0
end
