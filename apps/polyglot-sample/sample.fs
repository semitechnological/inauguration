let answer () = 42

let main _ = ()

type Counter(start: int) =
    let mutable value = start
    member this.inc () = value <- value + 1; value
    member this.get () = value
