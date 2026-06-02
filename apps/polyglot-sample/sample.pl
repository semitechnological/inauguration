sub answer {
    return 42;
}

sub main {
    my $value = answer();
    return;
}

package Counter;
sub new {
    my ($class, $start) = @_;
    my $self = {
        value => $start,
    };
    bless $self, $class;
    return $self;
}
sub inc {
    my ($self) = @_;
    $self->{value} += 1;
    return $self->{value};
}
